//! 按能力拆的四个小接口(探活/执行/采集/Issue·MR 操作)+ 基座 trait
//! `Connector`。明确禁止一个大 trait——design-s2-connector.md §3 的取舍表判定
//! 「枚举 + 手写上转方法」:对象安全、零 `dyn Any`、零 unsafe;声明由实现推导,
//! 分叉不可能;新增能力=基座加一个 default 方法,老适配器不动。

use async_trait::async_trait;

use crate::contract::{
    CallCtx, Capability, CapabilitySet, ConnResult, ConnectorKind, ExecSpec, ExecState, ExecTicket,
    IdemKey, ProjectBinding, WriteOutcome,
};

/// 所有连接器的基座。**它自己不含任何业务方法**——业务全在四个小接口里。
/// 基座只回答两件事:你是谁、你能干哪几类事。
pub trait Connector: Send + Sync {
    /// 契约协议版本。适配器写死 `contract::PROTOCOL`;注册时不匹配当前版本
    /// 的一律拒绝登记(而不是运行时才炸)。
    fn protocol(&self) -> u32 {
        crate::contract::PROTOCOL
    }

    fn kind(&self) -> &ConnectorKind;
    fn binding(&self) -> &ProjectBinding;

    // ─── 能力声明 = 手写上转,不用 Any,不用 downcast ───
    fn as_probe(&self) -> Option<&dyn Probe> {
        None
    }
    fn as_execute(&self) -> Option<&dyn Execute> {
        None
    }
    fn as_collect(&self) -> Option<&dyn Collect> {
        None
    }
    fn as_issue_ops(&self) -> Option<&dyn IssueOps> {
        None
    }

    /// **provided 方法,由上面四个推导出来**——声明与实现不可能分叉,因为
    /// `as_probe` 返回 `Some(self)` 编译器要求 `Self: Probe`。界面上「这条
    /// 连接器支持什么」直接读它,不需要第二处维护。
    fn capabilities(&self) -> CapabilitySet {
        let mut s = CapabilitySet::EMPTY;
        if self.as_probe().is_some() {
            s = s.with(Capability::Probe);
        }
        if self.as_execute().is_some() {
            s = s.with(Capability::Execute);
        }
        if self.as_collect().is_some() {
            s = s.with(Capability::Collect);
        }
        if self.as_issue_ops().is_some() {
            s = s.with(Capability::IssueOps);
        }
        s
    }
}

// ─── ① 探活 ───────────────────────────────────────────────────────────────

/// 探活能力:真的连上去问一句,拿到回答才算连上。
#[async_trait]
pub trait Probe: Send + Sync {
    /// 成功返回一行人话详情(如 `owner/repo · private · 最近推送 2026-08-09`)
    /// 与结构化身份;失败按契约的七类分类如实报。**绝不因为「CLI 装了」就
    /// 返回成功。**
    async fn probe(&self, cx: &CallCtx) -> ConnResult<ProbeReport>;
}

/// 一次探活的结果。
///
/// 刻意没有 `reachable: bool` 字段——探活失败走 `Err`,能拿到 `ProbeReport`
/// 就已经证明连上了;恒真字段不携带信息,删掉它比留着「以防将来」更诚实。
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeReport {
    /// 上游认定的身份,如 `"owner/repo"` / `"org/repo"` / 脚本绝对路径。
    pub identity: String,
    /// 一行人话,直接可上界面。
    pub detail: String,
}

// ─── ② 执行(切片三 agentcli 层填,本片只定型)──────────────────────────

/// 执行能力:起一次 agent 类 CLI 执行。切片三(agentcli 层,在 `bw-engine`
/// 里实现)填内容,本片只定型接口。
#[async_trait]
pub trait Execute: Send + Sync {
    /// 起一次执行。**只起,不等**——等是运行管理器的事(切片四)。
    async fn start(&self, cx: &CallCtx, spec: ExecSpec) -> ConnResult<ExecTicket>;
    /// **轮询**当前状态(不建订阅/游标/重放)。
    async fn poll(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<ExecState>;
    /// 取消。幂等:已结束的 ticket 取消 = `Ok`。
    async fn cancel(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<()>;
}

// ─── ③ 采集 ───────────────────────────────────────────────────────────────

/// 采集能力:跑一次采集,交回原始数据。
#[async_trait]
pub trait Collect: Send + Sync {
    /// 跑一次采集,交回**原始 JSON**。按字段路径取值(`collect_query`)是
    /// 指标域的事,连接器不解释语义——沿用「采集器从不解释」的老规矩。
    /// 采不到就 `Err`,**绝不返回 0 冒充「采到了 0」**(假零是本仓库的红线)。
    async fn collect(&self, cx: &CallCtx, req: CollectReq) -> ConnResult<CollectOut>;
}

/// 一次采集请求。主控裁决 #12:一个方法 + enum 返回,不拆两个方法(能力矩阵
/// 不膨胀)。
pub enum CollectReq {
    /// 脚本连接器:跑脚本 → 读 output 文件 → 整份 JSON 回来。
    ScriptRun,
    /// 仓连接器:一条计数查询(gh `search/issues` total_count / codehub
    /// list length)。
    RemoteCount { query: String, today: time::Date },
}

/// 一次采集的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct CollectOut {
    /// `ScriptRun` 给整份 JSON;`RemoteCount` 给 `Number`。
    pub value: serde_json::Value,
    /// 「这个数从哪来的」——证据链要点开原始出处。
    pub source_hint: String,
}

// ─── ④ Issue·MR 操作 ────────────────────────────────────────────────────

/// Issue·MR 操作能力:建活、查状态、提变更、合入。
#[async_trait]
pub trait IssueOps: Send + Sync {
    /// 建一个 Issue。**如实标注不对称**(见 [`IdemKey`] 文档):这里的
    /// `IdemKey` 只作日志追溯,不是真防重——按标题查重不可靠。
    async fn create_issue(
        &self,
        cx: &CallCtx,
        idem: IdemKey,
        title: &str,
        body: &str,
    ) -> ConnResult<WriteOutcome<u32>>;
    async fn issue_state(&self, cx: &CallCtx, number: u32) -> ConnResult<IssueState>;
    async fn close_issue(&self, cx: &CallCtx, idem: IdemKey, number: u32) -> ConnResult<()>;

    /// 提 MR/PR:内部会 stage+commit+push 活分支(用**内建**工作区函数,那些
    /// 函数不出现在本接口上)。永不 merge。这里的 `IdemKey` 是真防重——按
    /// head 分支查已开的 MR/PR 是天然锚点。
    async fn open_change(
        &self,
        cx: &CallCtx,
        idem: IdemKey,
        req: OpenChangeReq,
    ) -> ConnResult<WriteOutcome<u32>>;
    async fn change_state(&self, cx: &CallCtx, number: u32) -> ConnResult<ChangeState>;
    /// 找某分支上已开的 MR/PR(飘移巡检:队友自己开的 PR 也要认出来)。
    async fn open_change_for_branch(&self, cx: &CallCtx, branch: &str) -> ConnResult<Option<u32>>;
    /// **人点的那一下**。只能由编排层的显式合入命令调,任何执行路径不许调。
    async fn merge_change(&self, cx: &CallCtx, idem: IdemKey, number: u32) -> ConnResult<()>;

    /// 门禁检查结果。v1 两家都没有对应实现——默认如实返回「不支持」,谁先
    /// 补上谁 override,**不给假空列表**(空列表会被读成「检查全过」)。
    async fn checks(&self, cx: &CallCtx, _number: u32) -> ConnResult<Vec<CheckRun>> {
        Err(crate::contract::Fail {
            req: cx.req,
            took: std::time::Duration::ZERO,
            err: crate::contract::ConnError::Unsupported {
                cap: Capability::IssueOps,
                op: "checks",
            },
        })
    }
}

/// 提 MR/PR 需要的信息(分支、标题、正文——具体字段由适配器在收编时补齐,
/// 这里先留最小形状,骨架阶段字段以能编译通过为准)。
pub struct OpenChangeReq {
    /// 源分支(活分支)。
    pub branch: String,
    /// 目标分支(MR/PR 要合进哪条)。
    pub base: String,
    pub title: String,
    pub body: String,
}

/// Issue 状态归一化:上游 `OPEN`/`CLOSED`/`opened`/`closed` 这些原生词只活在
/// 适配器里,交回内核的是这两档。
#[derive(Debug, Clone, PartialEq)]
pub enum IssueState {
    Open,
    Closed,
}

/// MR/PR 状态归一化:同上,`Merged` 是三档里唯一不可逆的一档。
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeState {
    Open,
    Merged,
    Closed,
}

/// 一条门禁检查结果(`gh pr checks` 那一行的归一化形状)。
pub struct CheckRun {
    pub name: String,
    pub conclusion: CheckConclusion,
    pub url: String,
}

/// 门禁检查的结论。`Unknown` 是如实的「问不出来」,不是「过了」。
pub enum CheckConclusion {
    Passed,
    Failed,
    Running,
    Unknown,
}
