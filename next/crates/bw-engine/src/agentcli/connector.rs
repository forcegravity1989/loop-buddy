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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bw_connector::caps::{Connector, Execute, Interactive, Probe, ProbeReport, TermInput};
use bw_connector::contract::{
    guarded, CallCtx, ConnError, ConnResult, ConnectorEntry, ConnectorKind, ExecSpec, ExecState,
    ExecTicket, InjectBlock, OpClass, ProjectBinding, RequestId, SessionEnd,
};
use bw_core::playbook::PlaybookCtx;
use bw_core::{ConversationId, IssueId, WorkflowId};
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use super::pump::spawn_pump;
use super::session::{discover_sessions, InjectRecord, SessionRow, SessionTable};
use crate::interactive_cli::{
    build_bridge_system_prompt, build_resume_plan, build_startup_plan, InteractiveExecutor,
    PtyInput, TuiAgentConfig,
};
use crate::terminal_manager::{ConversationMeta, TerminalManager};
use crate::{ExecError, RunCtx};

/// [`TermInput`](bw-connector 契约层的镜像声明,见该类型文档)→
/// [`PtyInput`](这一层内部真正喂给 `TerminalManager::input` 的类型)。两
/// 个变体一一对应,纯搬运,不带判断。
impl From<TermInput> for PtyInput {
    fn from(input: TermInput) -> Self {
        match input {
            TermInput::Bytes(b) => PtyInput::Bytes(b),
            TermInput::Resize { cols, rows } => PtyInput::Resize { cols, rows },
        }
    }
}

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

/// next 切片三-2 修(I1)构造喂给 [`build_bridge_system_prompt`] 的
/// [`PlaybookCtx`]。**走「决策规则」里的路 A,不改 `ExecSpec` 契约形状**:
///
/// `build_bridge_system_prompt` 要九个字段(项目名/类型/说明/对标/机会/北
/// 极星/北极星定义/交棒记录/工作区提示)。agentcli 层眼下手里只有
/// `ExecSpec.workspace`/`branch`(经这个函数的两个形参传入)是**真实数
/// 据**——项目名、描述、对标、机会、北极星、交棒记录全部活在 bw-app 的
/// Issue/Project 领域模型里,agentcli 层按设计不依赖 store(`agentcli` 模块
/// 文档「必须保住的一条性质」:编排层只依赖 `bw-connector`,`bw-engine` 这
/// 层同样够不到那份数据)。这七个字段如实填「(未提供)」——**不留空**:
/// `build_bridge_system_prompt` 对空字符串字段是条件渲染(悄悄跳过不打
/// 印),读的人会误以为「这类上下文本来就没有」,而事实是「这一层接不
/// 到」,两者不是一回事,「(未提供)」把这个差别说清楚。`skill_slug` 传空
/// 串——`build_bridge_system_prompt` 本身已经支持这个输入,渲染成「未关联
/// 技能,由你驱动或按用户要求干活」,同样是如实,不是取巧绕过。
///
/// **为什么不走路 B(给 `ExecSpec` 加可选 ctx 字段,协议号 2→3)**:
/// `build_bridge_system_prompt` 的「通用铁律」段落(完成永不自动 / settle-
/// once / Signal derive-only / 合并永远是人 / 禁 `gh pr merge`)是**无条件
/// 渲染**的,不依赖任何 ctx 字段是否填了——哪怕九个字段有七个是
/// 「(未提供)」,I1 真正要保住的那句铁律正文照样一字不少地进系统提示词。
/// 项目上下文那部分即使残缺,每一处缺口都显式标了「(未提供)」,不是留白
/// 让人瞎猜,没有触发「决策规则」里「明显残缺误导」的升级条件。真要把项目
/// 上下文接进来,是切片四编排层把 Issue 真正交给 `Execute::start` 的那
/// 天——那时候的契约决定权也该在切片四,不该 agentcli 层这一片单方面拍板
/// 扩契约。
fn bridge_prompt_ctx(workspace: &Path, branch: &str) -> PlaybookCtx {
    let unknown = || "(未提供)".to_string();
    PlaybookCtx {
        project_name: unknown(),
        project_kind: unknown(),
        project_desc: unknown(),
        benchmark: unknown(),
        opportunity: unknown(),
        north_star: unknown(),
        ns_def: unknown(),
        handoff_note: unknown(),
        // 唯一两个真实可得的字段——直接来自 `ExecSpec`,不是编的。
        workspace_hint: format!("{}(分支 {branch})", workspace.display()),
    }
}

/// 装配一次首启的系统提示词:**衔接层正文(design §6.1/§9 明文的既定顺
/// 序)在前,`ExecSpec.inject` 按序在后**。next 切片三-2 修(I1)之前这里
/// 只拼了 `inject`,`build_bridge_system_prompt` 的正文(含「完成永不自动/
/// 绝不伪造观测/合并永远是人」等通用铁律)从没接进去——commit D/E 的报告
/// 与本函数旧文档都说这是「本片的职责只有一件:忠实搬运」,但实际漏了设计
/// 明文要求的这一半,登记在本轮修复清单 I1,不是新发现的缺口,是补齐当时
/// 漏做的接线。
///
/// `inject` 记账口不变:每块记标签/字节数/sha256 前 8 位([`InjectRecord`]),
/// **顺序等于调用方给的顺序,一个字不改、不截断、不重排**;`inject` 为空
/// 时,系统提示词仍然非空(衔接层正文恒非空),但记账清单为空——如实,不
/// 假装注入了技能块(§6.2「空就是空」,这条只管 `inject` 这一半,不管衔接
/// 层正文本身是否存在)。
///
/// `pub`:切片三E 的 `agent_session` 指挥器要独立调这个函数,把落盘的系统
/// 提示词与 `SessionRow.injected` 交叉核对(§7.2 第 6 条断言)。
pub fn assemble_system_prompt(
    workspace: &Path,
    branch: &str,
    blocks: &[InjectBlock],
) -> (String, Vec<InjectRecord>) {
    let ctx = bridge_prompt_ctx(workspace, branch);
    let mut prompt = build_bridge_system_prompt(&ctx, "");
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
    /// next 切片 5.5:每个活着的会话一条广播频道,`start` 在 `attach` 的同
    /// 一刻开一条、`close_conversation` 在会话收尾时摘掉——
    /// `Interactive::subscribe` 从这张表按会话 id 发一份 `Receiver`;
    /// [`pump::spawn_pump`] 起的泵任务按 v1 同款 100ms 节奏抽
    /// `TerminalManager::drain_events`,再按会话 id 分发进这张表对应的频
    /// 道。**两张表键不同、锁分开**(同本文件顶部模块文档「两张表,键不
    /// 同,不绑一起」的既有原则,这里加的是第三张,同一个原则)。
    byte_senders: Arc<Mutex<HashMap<ConversationId, broadcast::Sender<Vec<u8>>>>>,
}

/// 每会话广播频道的容量——同 [`crate::terminal_manager::OUTPUT_BATCH_CAP`]
/// (输出环的批次上限)同一个数量级:订阅方掉线/迟迟不 `recv` 时,
/// `broadcast::Sender::send` 不阻塞、只让最老的批次被挤掉(`Lagged`),不
/// 会拖慢泵任务或撑爆内存。
const BYTE_CHANNEL_CAPACITY: usize = 64;

impl AgentCliConnector {
    pub fn new(
        binding: ProjectBinding,
        row: &'static TuiAgentConfig,
        exec: Arc<dyn InteractiveExecutor>,
    ) -> Self {
        let terminals = Arc::new(Mutex::new(TerminalManager::new()));
        let byte_senders = Arc::new(Mutex::new(HashMap::new()));
        // next 切片 5.5/主控裁决 7:字节泵住在这里(引擎侧),不住壳
        // 里——见 `pump` 模块文档。这个连接器实例活多久,泵就跑多久
        // (`pump::spawn_pump` 文档「不做显式停止」一节如实记了这条取
        // 舍)。
        spawn_pump(terminals.clone(), byte_senders.clone());
        Self {
            kind: ConnectorKind::AgentCli {
                cli: row.slug.to_string(),
            },
            binding,
            row,
            exec,
            terminals,
            sessions: Arc::new(Mutex::new(SessionTable::new())),
            byte_senders,
        }
    }

    /// 关掉一个会话的两张表:`TerminalManager` 里的 PTY 路由 + 这个连接器
    /// 自己的广播频道。**两处必须一起摘**——只摘前者会让 `byte_senders`
    /// 里留一条再也不会有新字节、却还没被 drop 的频道(不是内存泄漏,
    /// `broadcast::Sender` 的订阅方会在下次 `recv` 时收到 `Closed`
    /// 才发现——`Sender` 本身没有被 drop,`Closed` 永远不会发生,是一条
    /// 真实的行为偏差,不只是"不好看")。原来 `terminals.lock().close(id)`
    /// 的两处调用点(开工外呼后台任务收尾 / `cancel`)都改叫这个函数,零
    /// 逻辑改写,只是把两行合成一步不许再漏掉一半。
    ///
    /// **关联函数、不是 `&self` 方法**——两处调用点都在 `tokio::spawn` 出
    /// 去的 `'static` 任务里(后台开工任务本身、`cancel` 内部的
    /// `guarded(...)` 异步块),它们持有的是 `terminals`/`byte_senders`
    /// 两个 `Arc` 的克隆,不持有 `&self`(`self` 的生命周期不是
    /// `'static`,借不进 `tokio::spawn`)。
    fn close_conversation(
        terminals: &Mutex<TerminalManager>,
        byte_senders: &Mutex<HashMap<ConversationId, broadcast::Sender<Vec<u8>>>>,
        id: ConversationId,
    ) {
        {
            let mut t = terminals.lock().expect("terminal manager mutex poisoned");
            t.close(id);
        }
        let mut senders = byte_senders.lock().expect("byte senders mutex poisoned");
        senders.remove(&id);
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

    /// 供指挥器读回用:按工作区路径查这条会话在 `TerminalManager` 里记的
    /// 真实尺寸(`current_size`)——不是契约的一部分,只给
    /// `terminal_feed_smoke` 核对 `send_input(Resize)` 真的把尺寸账目写回
    /// 了管理器(评审 task-s55-review.md Important-1 的修复轮新增)。工作
    /// 区路径复用的是 `SessionTable::find_by_workspace` 这条既有索引(续
    /// 接判定唯一入口),不新开一条平行的按名查找。
    pub fn terminal_size_by_workspace(&self, workspace: &Path) -> Option<(u16, u16)> {
        let conversation = self
            .sessions
            .lock()
            .expect("session table mutex poisoned")
            .find_by_workspace(workspace)?
            .conversation;
        self.terminals
            .lock()
            .expect("terminal manager mutex poisoned")
            .current_size(conversation)
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

    /// next 切片 5.5(design-s5-hexpanel.md §5.3):claude 一家真支持交互式
    /// 字节流——它本来就是一次 PTY 会话,`Interactive` 只是把已经在
    /// `terminals`/`byte_senders` 里活着的东西经统一契约递出去。
    fn as_interactive(&self) -> Option<&dyn Interactive> {
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
        let byte_senders = self.byte_senders.clone();
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
            //
            // **M2(next 切片三-2 修,并发纪律留白如实标注)**:这一步与下面
            // `t.insert(...)` 之间没有一把贯穿的锁——两次 `sessions.lock()`
            // 是各自独立、快进快出的临界区(design §1.2「不跨 `.await` 持
            // 锁」的直接后果)。如果同一个工作区被并发 `start` 两次,两次调
            // 用都可能在这里读到「查不到会话」,都走首启分支,最终各自
            // `insert` 一行——`by_workspace` 会被后写的那次覆盖,先写的那
            // 行仍留在 `by_req` 里但再也查不到,等价于「同一个工作区意外起
            // 了两个会话」。design §8 明写「本片只保证只起不等和自己这张表
            // 的并发安全,不做同项目唯一交付的锁」——这条并发窗口的真正守
            // 卫(同项目/同工作区一次只许一个交付在跑)留给切片四编排层的
            // 运行管理器,agentcli 层这一片不单方面加锁。
            let found_in_table = {
                let table = sessions.lock().expect("session table mutex poisoned");
                table
                    .find_by_workspace(&workspace)
                    .map(|row| row.upstream_session.clone())
            };
            let discovered = found_in_table.filter(|s| !s.is_empty()).or_else(|| {
                discover_sessions(&workspace)
                    .into_iter()
                    .max_by_key(|(_, mtime)| *mtime)
                    .map(|(id, _)| id)
            });

            // **C1(next 切片三-2 修,本轮最关键的一条)**:首启且没有历史
            // 可续接时,`upstream_session` 该不该编一个 uuid,只看这家 CLI
            // 是不是真能被指派会话号(`row.session_id_flag.is_some()`)——之
            // 前这里无条件编一个 uuid 塞进 `SessionRow.upstream_session`/
            // 回传的 `ExecTicket`,但从没把它交给 claude(`build_startup_
            // plan` 当时压根不接收 `session_id` 参数),claude 会用自己生
            // 成的会话号落 jsonl,票据上的号是一句谎话,读回必空(见
            // `plan/23-opc-stitching-rebuild.md` §10 第 4 条的实测坐实)。
            // 现在:能指派就编号、真交给 claude;不能指派就如实留空
            // (`String::new()`),不猜——这是 `session.rs`
            // `SessionRow.upstream_session` 字段文档自己定的判据:「不能
            // 指派的家在这里留空,如实标注未知,绝不填一个猜的」。
            let (upstream_session, is_resume) = match discovered {
                Some(id) => (id, true),
                None if row.session_id_flag.is_some() => (uuid::Uuid::new_v4().to_string(), false),
                None => (String::new(), false),
            };

            let (plan, injected) = if is_resume {
                // §6.3:续接不重注入——复利块首启时已经在会话里,重注是污
                // 染,不是漏注。
                let plan = build_resume_plan(row, Some(&upstream_session), &workspace)
                    .map_err(exec_error_to_conn_error)?;
                (plan, Vec::new())
            } else {
                // I1(next 切片三-2 修):`assemble_system_prompt` 现在把
                // 衔接层正文拼在 `inject` 前面,见该函数文档。
                let (system_prompt, injected) =
                    assemble_system_prompt(&workspace, &required_branch, &inject);
                // C1:只有 `upstream_session` 真的非空(即上面判定「这家能
                // 被指派」)才把它交给 `build_startup_plan` 的 `session_id`
                // 形参;为空就传 `None`——`build_startup_plan` 自己也会做
                // 同样的判断,这里传 `None` 只是不制造一个「传了空字符
                // 串」的中间态,语义更直白。
                let session_id_arg = if upstream_session.is_empty() {
                    None
                } else {
                    Some(upstream_session.as_str())
                };
                let plan = build_startup_plan(
                    row,
                    GENERIC_KICKOFF_PROMPT,
                    &system_prompt,
                    session_id_arg,
                    &workspace,
                )
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
            // next 切片 5.5:这个会话专属的广播频道——`pump::spawn_pump`
            // 抽到这个会话的字节就是往这条频道发,`Interactive::subscribe`
            // 就是从这里 `.subscribe()` 一份。开在 `attach` 成功的同一刻,
            // 关在 `close_conversation`(下面两处收尾各自会调)。
            {
                let (tx, _rx) = broadcast::channel(BYTE_CHANNEL_CAPACITY);
                let mut senders = byte_senders.lock().expect("byte senders mutex poisoned");
                senders.insert(conversation_id, tx);
            }

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
            let terminals_bg = terminals.clone();
            let byte_senders_bg = byte_senders.clone();
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
                {
                    let mut t = sessions_bg.lock().expect("session table mutex poisoned");
                    if let Some(row) = t.get_mut(req) {
                        // 竞态守卫:`cancel` 可能已经把这一行标成
                        // Canceled——那一档一旦落定就不许被这里的「正常结
                        // 束」覆盖回去(人点的取消优先于事后才跑完的收尾)。
                        if row.state != ExecState::Canceled {
                            row.state = match outcome {
                                // I5(next 切片三-2 修):`ProcessExit.code`
                                // 之前恒 `None`——`SkillOutput` 当时还没有
                                // `exit_code` 字段。现在有了,顺手接上
                                // (`InteractiveExecutor::run_skill_pty` 两
                                // 个实现——真实 PTY 后端 / mock——各自决定
                                // 拿不拿得到,见 `SkillOutput::exit_code`
                                // 字段文档;拿不到就如实 `None`,这里不猜)。
                                Ok(output) => ExecState::Finished {
                                    ended: SessionEnd::ProcessExit {
                                        code: output.exit_code,
                                    },
                                    summary: output.summary,
                                },
                                // F3(评审 task-s3b-fixreview.md,next 切
                                // 片三-2 修二新增):这一臂吞了两种不同来
                                // 源的 `Err`,`ExecError` 只有一个
                                // `Failed(String)` 变体(`bw-engine/src/
                                // lib.rs`),类型层面区分不了——① spawn 即
                                // 失败,进程压根没起来(`pty_backend.rs`
                                // `openpty`/`spawn_command` 失败,`run_
                                // skill_pty` 直接返回 `Err`,我们确定它没
                                // 跑过);② 跑起来之后连接断了,去向不明
                                // (PTY 读循环/收尾异常)。契约不为这个区
                                // 分加新变体——两者都落
                                // `SessionEnd::ContactLost`,差别只在
                                // `summary`(= `e.to_string()`)的错误正文
                                // 里。读账的人要靠 `summary` 分辨这两种情
                                // 况,不要把 `ContactLost` 一律读成「它可
                                // 能还活着」:①的情况下它确定没活过。
                                Err(e) => ExecState::Finished {
                                    ended: SessionEnd::ContactLost,
                                    summary: e.to_string(),
                                },
                            };
                        }
                    }
                }
                // M1(next 切片三-2 修):会话自然结束(不是被 `cancel` 摘
                // 的)也要把 `TerminalManager` 里这一行摘掉——否则界面侧
                // `has_live()` 会把一个 PTY 子进程早就退出、只是没人显式
                // `cancel` 过的会话继续算成「活着」,是假活。
                // `TerminalManager::close` 对已经不存在的 key 是安全的
                // no-op(`HashMap::remove` 找不到就什么也不干),`cancel`
                // 已经摘过的会话这里重复摘一次没有副作用。next 切片
                // 5.5:同一步也要摘掉 `byte_senders` 里这一条(见
                // `close_conversation` 文档)。
                Self::close_conversation(&terminals_bg, &byte_senders_bg, conversation_id);
            });

            // C1:`upstream_session` 是空串时,票据如实回 `None`——不把一个
            // 「本来就没有」的字段包一层 `Some("")` 糊弄过去。
            let upstream_session = if upstream_session.is_empty() {
                None
            } else {
                Some(upstream_session)
            };
            Ok(ExecTicket {
                req,
                upstream_session,
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
        let byte_senders = self.byte_senders.clone();
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

            // next 切片 5.5:同一步摘掉 `byte_senders` 里这一条(见
            // `close_conversation` 文档)。
            Self::close_conversation(&terminals, &byte_senders, conversation);
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

/// next 切片 5.5(design-s5-hexpanel.md §5.3):`ExecTicket.req` → 会话表查
/// `conversation` → 广播频道表查这条会话的 `Sender`。**同步**——见
/// `bw_connector::caps::Interactive` 文档「不走 `guarded`/`CallCtx`」一
/// 节:这两个方法只是把已经在内存里活着的 channel 把手递出去,不联系任
/// 何外部系统。
impl Interactive for AgentCliConnector {
    fn subscribe(&self, ticket: &ExecTicket) -> Option<broadcast::Receiver<Vec<u8>>> {
        let conversation = self.session_row(ticket.req)?.conversation;
        let senders = self
            .byte_senders
            .lock()
            .expect("byte senders mutex poisoned");
        senders.get(&conversation).map(|tx| tx.subscribe())
    }

    fn send_input(&self, ticket: &ExecTicket, input: TermInput) -> bool {
        let Some(conversation) = self.session_row(ticket.req).map(|r| r.conversation) else {
            return false;
        };
        // next 切片 5.5 修(评审 task-s55-review.md Important-1):resize 必
        // 须走 `TerminalManager::resize`,不能跟字节输入一样只走
        // `input()`——v1 `bw-app/src/lib.rs` 的 `Command::TerminalResize` 就
        // 是这个分法(`terminal_manager.resize(...)`,失败时补
        // `note_fit_size`)。`resize()` 才会真的更新 `current_size`/
        // `last_fit_size` 两笔账(`terminal_manager.rs` 文档);这个连接器
        // `start()` 里 `attach` 读的 `last_fit_size()`(本文件上方「④ 起
        // PTY」那段)就是这笔账的唯一消费者——不走 `resize()`,它永远是
        // `None`,新会话永远从 80×24 起步,拿不到用户上一次真实调整过的
        // 终端尺寸。
        match input {
            TermInput::Resize { cols, rows } => {
                let mut terminals = self
                    .terminals
                    .lock()
                    .expect("terminal manager mutex poisoned");
                terminals.resize(conversation, cols, rows)
            }
            bytes @ TermInput::Bytes(_) => {
                let terminals = self
                    .terminals
                    .lock()
                    .expect("terminal manager mutex poisoned");
                terminals.input(conversation, bytes.into())
            }
        }
    }
}

fn exec_error_to_conn_error(e: ExecError) -> ConnError {
    ConnError::Other(e.to_string())
}
