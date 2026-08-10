//! 最小机器契约:协议版本、能力名、请求/防重编号、写操作结果、结构化错误分类、
//! 调用上下文,以及登记表用的连接器身份三件套(种类/项目绑定/本机配置引用)。
//! 契约钉在 trait 层——外部工具(gh/codehub-cli/……)的原生输出这里管不了,
//! 这里管的是适配器归一化后交回内核的东西必须长成的样子(design-s2-connector.md §4)。
//!
//! agentcli 层(切片三,在 `bw-engine` 里实现)也要经由本契约挂进注册表;本文件
//! 末尾的 `ExecSpec` / `ExecTicket` / `ExecState` 三个类型是它唯一要遵守的地基
//! (design §6)。`ExecState` 刻意没有 `Done` 变体——执行连接器再成功也只产出
//! `Finished`,把 Issue 推到「评审中」;「评审中」→「完成」的唯一入口是
//! `bw-core` 状态机的显式转移。产品铁律在契约类型上就成立,不靠纪律。

use std::path::PathBuf;
use std::time::Duration;

use bw_core::{ConnectorId, ProjectId};
use tokio_util::sync::CancellationToken;

/// 协议版本。改动契约类型的形状(增删字段、改语义)必须 +1;注册时
/// `conn.protocol() != PROTOCOL` 一律拒绝登记,不做兼容层
/// (CLAUDE.md:不为向后兼容留旧路径)。
///
/// **契约冻结点 = 切片二B 收编完成**:在那之前(骨架阶段到 gh/codehub/script
/// 三家真实收编期间)对契约类型的形状调整不撞版本号——冻结后再增删字段才必须
/// `PROTOCOL + 1`。
pub const PROTOCOL: u32 = 1;

/// 能力名。既是路由键,也是「不支持」错误里说得清的那个词。裁决 #11:通信
/// 能力(通知出口)不预先占位——空枚举项会诱导人往里填东西,做减法是纲。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    /// 探活:真的连上去问一句、拿到回答才算连上。
    Probe,
    /// 执行:起一次 agent 类 CLI 执行(切片三填)。
    Execute,
    /// 采集:跑一次采集,交回原始数据(指标域才解释语义)。
    Collect,
    /// Issue·MR 操作:建活、查状态、提变更、合入。
    IssueOps,
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Capability::Probe => "探活",
            Capability::Execute => "执行",
            Capability::Collect => "采集",
            Capability::IssueOps => "Issue·MR 操作",
        };
        write!(f, "{label}")
    }
}

/// 手搓的位集(不引 bitflags 依赖,四个位不值得一个 crate)。用于
/// `Connector::capabilities()` 的推导结果,给界面「这条连接器支持什么」用。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet(u8);

impl CapabilitySet {
    pub const EMPTY: Self = Self(0);

    pub const fn with(self, c: Capability) -> Self {
        Self(self.0 | (1 << (c as u8)))
    }

    pub const fn has(self, c: Capability) -> bool {
        self.0 & (1 << (c as u8)) != 0
    }
}

/// 请求编号——一次调用一个,进 stderr 诊断日志、进运行记账、进「待人处理」条目。
/// 出问题时能把「界面上这条红」和「当时那次 gh 调用」对上。
///
/// 内部字段私有:构造只走 [`RequestId::new`],不给外部塞自造的 uuid。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId(uuid::Uuid);

impl RequestId {
    /// 生成一个新请求编号(uuid v4)。每次调用一个,不复用。
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 防重编号——只有**可重试的写操作**要带(`create_issue` / `open_change` /
/// `merge_change` / `close_issue`)。读操作和采集不带。
///
/// 由调用方(运行管理器)**确定性**生成,同一件活的同一个动作永远同一个值,
/// 例:`issue-42/open-change`。重试时编号不变,这是「同一请求重发不做两遍」的锚。
///
/// **如实标注一处不对称**(主控裁决 #4):`open_change` 有天然的 read-before-write
/// 锚点(按 head 分支查已开的 MR/PR),`create_issue` 没有——按标题查重不可靠
/// (标题可重名)。首版接受这个不对称:`create_issue` 上的 `IdemKey` 只作日志
/// 追溯用,不是真防重;真正的防重只在 `open_change` 上成立。
///
/// 内部字段私有:格式钉死在 [`IdemKey::for_issue_action`] 一处,不给调用方
/// 各自拼字符串(拼法一旦分叉,重试就认不出是同一件事)。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IdemKey(String);

impl IdemKey {
    /// 构造一个防重编号。格式钉死 `issue-{issue_no}/{action}`——同一件活的
    /// 同一个动作永远同一个值,重试时不变。
    pub fn for_issue_action(issue_no: u64, action: &'static str) -> Self {
        Self(format!("issue-{issue_no}/{action}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 写操作的结构化结果。**这是防重的落地形态,也是唯一诚实的形态**:
/// gh / codehub-cli 都没有幂等头,唯一可靠的做法是「先读回、再决定」。
///
/// 与 [`ConnError::Timeout`] 同一口径:写操作超时不等于失败,更不等于「没
/// 发生」——运行管理器带着同一个 [`IdemKey`] 重试,重试经 read-before-write
/// 会落进 `AlreadyExisted`,账才对得上。
#[derive(Debug, Clone, PartialEq)]
pub enum WriteOutcome<T> {
    /// 这次调用真的创建了它。
    Created(T),
    /// 上游已经有了(可能是上一次重试建的,也可能是队友自己建的)——
    /// **号码是真读回来的**,绝不从错误文本里抠、绝不猜。
    AlreadyExisted(T),
}

/// 结构化失败分类。适配器负责把 gh/codehub 的原生错误映射到这七类
/// (`Unsupported` 算作一类独立的「不支持」,不算在下面六类失败里,合计七档)。
#[derive(Debug, thiserror::Error)]
pub enum ConnError {
    /// 该连接器压根没实现这个能力/操作——如实说不支持,不给假空结果。
    #[error("该连接器不支持{cap}的 {op} 操作")]
    Unsupported { cap: Capability, op: &'static str },
    /// CLI 不在 PATH、没登录、host 不可达——「装了≠连上了」的失败面。
    #[error("未连接:{0}")]
    NotConnected(String),
    /// **写操作超时 = 状态未知,不是失败**:超时不代表上游没建成(MR/Issue 可能
    /// 已经建好,只是回包没赶上)。绝不当失败记账——运行管理器必须带着同一个
    /// [`IdemKey`] 重试;重试经 read-before-write 会得到 `AlreadyExisted`,
    /// 账才对得上(design §4「超时与取消」第三条)。
    #[error("超时({0:?} 未回)")]
    Timeout(Duration),
    #[error("已取消")]
    Canceled,
    /// 连上了,上游明确说不行(权限不足、分支不存在、MR 已合……)。
    #[error("上游拒绝:{message}")]
    UpstreamRejected { message: String },
    /// 连上了、也回了,但回的东西解析不了——**这一类绝不降级成「没数据」**,
    /// 它是真故障,必须进「待人处理」。
    #[error("输出不可解析:{raw}")]
    Unparsable { raw: String },
    #[error("其它:{0}")]
    Other(String),
}

/// 四个能力小接口的统一返回形状:成功走 [`CallOk`],失败走 [`Fail`]——两边都带
/// 请求编号与耗时,证据链要能点开原始出处。
pub type ConnResult<T> = Result<CallOk<T>, Fail>;

/// 成功也带元数据——证据链要能点开原始出处。
#[derive(Debug)]
pub struct CallOk<T> {
    pub req: RequestId,
    pub took: Duration,
    pub value: T,
}

/// 失败也带编号——「待人处理」的一条要能追回是哪次调用。
#[derive(Debug)]
pub struct Fail {
    pub req: RequestId,
    pub took: Duration,
    pub err: ConnError,
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.err)
    }
}

impl std::error::Error for Fail {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.err)
    }
}

/// 每次调用的上下文。四个小接口的每个方法首参都是它——这是统一的,不给例外。
pub struct CallCtx {
    pub req: RequestId,
    /// 相对超时。`None` = 用调用所属 [`OpClass`] 的默认档
    /// (见 [`OpClass::default_timeout`])。
    pub timeout: Option<Duration>,
    /// 取消令牌。
    pub cancel: CancellationToken,
}

/// 操作类别——决定 [`guarded`] 在 `CallCtx.timeout` 为 `None` 时用哪档默认
/// 超时(design §4「超时与取消」表)。五档钉在类型上,不是散落各处的裸数字。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpClass {
    /// 探活。10s——快失败,界面上要立刻显灰而不是转圈。
    Probe,
    /// 读(`issue_state` / `change_state` / `open_change_for_branch`)。
    /// 20s——一次 API 往返。
    Read,
    /// 采集 · `RemoteCount`。30s——搜索接口慢一档。
    CollectCount,
    /// 采集 · `ScriptRun`。180s——项目脚本可能跑 Playwright/SSO,给宽。
    CollectScript,
    /// 写(`create_issue` / `open_change` / `merge_change`)。60s——含 push,
    /// 大仓要时间。
    Write,
}

impl OpClass {
    /// 该操作类别的默认超时档。
    pub const fn default_timeout(self) -> Duration {
        match self {
            OpClass::Probe => Duration::from_secs(10),
            OpClass::Read => Duration::from_secs(20),
            OpClass::CollectCount => Duration::from_secs(30),
            OpClass::CollectScript => Duration::from_secs(180),
            OpClass::Write => Duration::from_secs(60),
        }
    }
}

/// 超时/取消的统一实现——**适配器不各自写 timeout**,只写业务 future,这两条
/// 边由这里兜(design §4「超时与取消」三条语义的第一条)。`op` 决定 `CallCtx`
/// 没给超时时用哪档默认([`OpClass::default_timeout`]),`CallCtx.timeout` 为
/// `Some` 时覆盖档位默认。
///
/// **取消义务在适配器那边,包装器兜不住**:适配器起子进程必须
/// `.kill_on_drop(true)`——`tokio::select!` 落选分支被 drop 只是让 future 停止
/// 被 poll,不会自动杀掉里面已经 spawn 的子进程;不加这个 flag,取消只是
/// BW 这边假装取消了,子进程仍在跑。这条义务写在这里,因为包装器本身看不见
/// 适配器 future 内部起了什么进程,补不了这个洞。
///
/// pub(跨 crate 可见,不是 `pub(crate)`):将来 `bw-engine` 的 agentcli 适配器
/// (切片三)也要用同一份超时/取消实现,不允许各自抄一份。
///
/// **2026-08-10 现状**:gh/codehub 两家收编自 v1 的冻结上游函数体,均未设
/// `.kill_on_drop(true)`——取消/超时对它们只切断 BW 侧等待,子进程可能续跑到
/// 自然结束;新写的适配器(agentcli 等)必须兑现本义务。
pub async fn guarded<T, F>(cx: &CallCtx, op: OpClass, fut: F) -> ConnResult<T>
where
    F: std::future::Future<Output = Result<T, ConnError>>,
{
    let d = cx.timeout.unwrap_or_else(|| op.default_timeout());
    let started = std::time::Instant::now();
    let outcome = tokio::select! {
        _ = cx.cancel.cancelled() => Err(ConnError::Canceled),
        _ = tokio::time::sleep(d) => Err(ConnError::Timeout(d)),
        r = fut => r,
    };
    let took = started.elapsed();
    match outcome {
        Ok(value) => Ok(CallOk {
            req: cx.req,
            took,
            value,
        }),
        Err(err) => Err(Fail {
            req: cx.req,
            took,
            err,
        }),
    }
}

/// 直接构造一个「不支持」的失败结果,**不经过 [`guarded`]**——这类失败是
/// 本地决定(连接器压根没实现这个操作),不涉及外呼,不需要计时/取消这两条
/// 边。[`crate::caps::IssueOps::checks`] 的默认实现也直接调这个函数——共享
/// 一处,不各自手写;给切片二B 起「上游真没有对应能力」的分支复用(如
/// codehub 没有 `issue_state`/`close_issue`/`change_state` 的上游函数,
/// 两家都没有脚本采集能力)。
pub fn unsupported<T>(cx: &CallCtx, cap: Capability, op: &'static str) -> ConnResult<T> {
    Err(Fail {
        req: cx.req,
        took: Duration::ZERO,
        err: ConnError::Unsupported { cap, op },
    })
}

/// 连接器种类。按**能力家族**分,不按厂商分。字符串形态只在存库/读文件时出现,
/// 进内存立刻收敛成这个枚举——编排层永远看不到 `"github"` 这种裸串。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConnectorKind {
    /// 仓连接器:gh。
    GithubRepo,
    /// 仓连接器:codehub-cli。
    CodehubRepo,
    /// 采集连接器:项目仓里的采集脚本(`.bw/connectors.toml` 是正本)。
    Script,
    /// 执行连接器:agent 类 CLI,由 agentcli 层实现(切片三填,本片只留位)。
    AgentCli { cli: String },
}

/// 项目绑定身份——「这条连接器代表这个项目在上游的哪个位置」。三家都能装得下:
/// github 的 `host="github.com"` / `path="owner/repo"`,codehub 的
/// `host="green"/"yellow"/内源域名` / `path="org/repo"`,script 的
/// `host=""` / `path=工作区根的绝对路径`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectBinding {
    pub project: ProjectId,
    pub host: String,
    pub path: String,
}

/// 本机配置引用——**只存引用,不存值**(凭证只放系统钥匙串或本机配置,不进
/// 项目仓不进日志)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigRef {
    /// 靠这个 CLI 自己的登录态(`gh auth login` / `codehub-cli auth login`)。
    /// 值就是 PATH 上的可执行名。BW 一个字节的凭证都不碰。
    ///
    /// **2026-08-10 现状**:gh/codehub 两家适配器目前不消费 `bin`(可执行名
    /// 硬编码在冻结上游体内为 `gh` / `codehub-cli`);`bin` 字段对它们无效,
    /// 将来解冻上游或新适配器接入时才生效。
    CliLogin { bin: String },
    /// 采集脚本:相对工作区根的脚本路径 + 运行命令 + 输出文件。三个字段直接
    /// 对应 `.bw/connectors.toml` 的 script/command/output。
    Script {
        script: String,
        command: String,
        output: String,
    },
    /// agentcli:注册表里的那一行(切片三的 agent 配置的 slug)。
    AgentRegistryRow { slug: String },
}

/// 一条连接器登记:种类 + 项目绑定身份 + 本机配置引用。三样够了,不做四层身份。
#[derive(Clone, Debug)]
pub struct ConnectorEntry {
    pub id: ConnectorId,
    /// 项目内唯一,脚本连接器沿用 `.bw/connectors.toml` 的 `name`。
    pub name: String,
    pub kind: ConnectorKind,
    pub binding: ProjectBinding,
    pub config: ConfigRef,
}

/// 起一次执行要给的最小信息。PTY、hook、session.jsonl、prompt 注入模式全部是
/// agentcli 层内部的事,**一个字都不进契约**。
pub struct ExecSpec {
    pub workspace: PathBuf,
    pub branch: String,
    /// 要注入的正文块(技能正文/蒸馏块/目录块)。契约只知道「有一段文本要送进
    /// 去」,不知道它是走 argv 还是走系统提示词——那是注册表那一行的事。
    pub inject: Vec<InjectBlock>,
    pub budget_usd: Option<f64>,
}

/// 一段要注入执行会话的正文块。
pub struct InjectBlock {
    pub label: String,
    pub body: String,
}

/// 一次执行的句柄。**上游会话号是核心**:agent CLI 的 resume 靠它。
pub struct ExecTicket {
    pub req: RequestId,
    pub upstream_session: Option<String>,
}

/// 轮询回来的状态。刻意只有五档——过程细节留在上游 CLI 里。
///
/// **刻意的类型断路**:这里没有 `Done` 变体。执行连接器再成功也只产出
/// `Finished`,把 Issue 推到「评审中」;从「评审中」到「完成」的唯一入口是
/// `bw-core` 状态机的显式转移。产品铁律在契约类型上就成立,不靠纪律,不许妥协。
#[derive(Debug, Clone, PartialEq)]
pub enum ExecState {
    Running,
    /// 干完了。**最远只能推到「评审中」**。
    Finished {
        ok: bool,
        summary: String,
    },
    Canceled,
    /// 上游会话还在,但 BW 这边重启了——如实标注,不假活。
    Orphaned,
    Unknown,
}
