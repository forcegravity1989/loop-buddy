//! codehub-cli(内源仓)连接器适配器(feature = "codehub")。整体收编 v1
//! `crates/bw-engine/src/codehub.rs`(见 `crate::upstream::codehub`,函数体零
//! 改写)——薄包装:trait 实现 + [`guarded`] 接线 + 错误归一化。
//! design-s2-connector.md §5「收编映射:v1 codehub.rs(codehub-cli)」逐条
//! 对照。
//!
//! **v1 codehub.rs 本身就没有 `issue_state`/`close_issue`/「查 MR 状态」的
//! 上游函数**(不是本次收编漏了,是 v1 从来没实现过——design §5 的映射表
//! 也只列了 `create_issue`/`create_mr`/`open_mr_for_branch`/`merge_mr`
//! 四个 IssueOps 方法,没提这三个)。`IssueOps::issue_state`/`close_issue`/
//! `change_state` 在这里如实返回 `Unsupported`,不假装有对应上游实现。

use std::sync::Arc;

use async_trait::async_trait;

use crate::caps::{
    ChangeState, Collect, CollectOut, CollectReq, Connector, IssueOps, IssueState, OpenChangeReq,
    Probe, ProbeReport,
};
use crate::contract::{
    guarded, unsupported, CallCtx, Capability, ConnError, ConnResult, ConnectorEntry,
    ConnectorKind, IdemKey, OpClass, ProjectBinding, WriteOutcome,
};
use crate::upstream::codehub as up;

/// codehub-cli 连接器。`binding.host` = API host 别名(`open`/`green`/
/// `yellow`);`binding.path` = `path_with_namespace`(如
/// `z30026659/my-service`)。
pub struct CodehubConnector {
    kind: ConnectorKind,
    binding: ProjectBinding,
}

impl CodehubConnector {
    pub fn new(binding: ProjectBinding) -> Self {
        Self {
            kind: ConnectorKind::CodehubRepo,
            binding,
        }
    }

    /// 登记工厂用的构造入口,同 [`crate::adapters::gh::GhConnector::from_entry`]
    /// 的口径:kind 对不上是装配期编码错误,`panic!`,不是运行时可恢复状况。
    pub fn from_entry(entry: &ConnectorEntry) -> Arc<dyn Connector> {
        assert_eq!(
            entry.kind,
            ConnectorKind::CodehubRepo,
            "CodehubConnector::from_entry 收到非 CodehubRepo 登记(name={:?},kind={:?})——\
             composition root 的装配期编码错误",
            entry.name,
            entry.kind
        );
        Arc::new(Self::new(entry.binding.clone()))
    }

    fn host(&self) -> &str {
        &self.binding.host
    }

    fn path(&self) -> &str {
        &self.binding.path
    }
}

impl Connector for CodehubConnector {
    fn kind(&self) -> &ConnectorKind {
        &self.kind
    }

    fn binding(&self) -> &ProjectBinding {
        &self.binding
    }

    fn as_probe(&self) -> Option<&dyn Probe> {
        Some(self)
    }

    fn as_collect(&self) -> Option<&dyn Collect> {
        Some(self)
    }

    fn as_issue_ops(&self) -> Option<&dyn IssueOps> {
        Some(self)
    }
}

/// `CodehubError` → 七档结构化分类。集中在这一处(主控裁决 #6)。
///
/// codehub 的错误类型比 github 多分了一档(`Parse`),这条映射是**结构性**的
/// (精确匹配 v1 上游代码里已经分好的枚举 variant,不是字符串嗅探,不需要
/// 验证日期):
/// - `NotInstalled` → `NotConnected`(codehub-cli 本机确认未安装,`which
///   codehub-cli` 2026-08-10 现场核实为空,「装了≠连上了」的最基础一档)。
/// - `Parse` → `Unparsable`(连上了、也回了,回的东西解析不了——上游自己
///   已经分好类,这里只是搬过来)。
///
/// `Command(String)` 同 github 适配器:2026-08-10 在本机无法安全现场核验
/// codehub-cli 真实的鉴权失败/仓不存在等 stderr 文案(codehub-cli 本身未
/// 装在本沙箱,且 v1 codehub-cli 是内源工具,本机也没有可连的内源网络)——
/// 按裁决 #6 落 `UpstreamRejected` 原文透传,不落未经验证的字符串规则。
fn classify(err: up::CodehubError) -> ConnError {
    match err {
        up::CodehubError::NotInstalled => {
            ConnError::NotConnected("codehub-cli 未安装或不在 PATH".into())
        }
        up::CodehubError::Command(msg) => ConnError::UpstreamRejected { message: msg },
        up::CodehubError::Parse(msg) => ConnError::Unparsable { raw: msg },
    }
}

#[async_trait]
impl Probe for CodehubConnector {
    /// design §5:`probe` → `Probe::probe`,一行人话直接进
    /// `ProbeReport.detail`。真的跑一次 `codehub-cli project view`。
    async fn probe(&self, cx: &CallCtx) -> ConnResult<ProbeReport> {
        let host = self.host().to_string();
        let path = self.path().to_string();
        let identity = path.clone();
        guarded(cx, OpClass::Probe, async move {
            up::probe(&host, &path)
                .await
                .map(|detail| ProbeReport { identity, detail })
                .map_err(classify)
        })
        .await
    }
}

#[async_trait]
impl Collect for CodehubConnector {
    /// design §5:`collect_count` → `Collect::collect(RemoteCount)`。
    /// codehub 不做脚本采集——`ScriptRun` 如实 `Unsupported`。
    async fn collect(&self, cx: &CallCtx, req: CollectReq) -> ConnResult<CollectOut> {
        match req {
            CollectReq::ScriptRun => unsupported(cx, Capability::Collect, "script_run"),
            CollectReq::RemoteCount { query, today } => {
                let host = self.host().to_string();
                let path = self.path().to_string();
                guarded(cx, OpClass::CollectCount, async move {
                    up::collect_count(&host, &path, &query, today)
                        .await
                        .map(|n| CollectOut {
                            value: serde_json::Value::from(n),
                            source_hint: format!("codehub-cli · {host}/{path} · {query}"),
                        })
                        .map_err(classify)
                })
                .await
            }
        }
    }
}

#[async_trait]
impl IssueOps for CodehubConnector {
    /// design §5:`create_issue` → `IssueOps::create_issue`。同 github:
    /// 无 read-before-write,`WriteOutcome` 恒 `Created`(主控裁决 #4)。
    async fn create_issue(
        &self,
        cx: &CallCtx,
        _idem: IdemKey,
        title: &str,
        body: &str,
    ) -> ConnResult<WriteOutcome<u32>> {
        let host = self.host().to_string();
        let path = self.path().to_string();
        let title = title.to_string();
        let body = body.to_string();
        guarded(cx, OpClass::Write, async move {
            up::create_issue(&host, &path, &title, &body)
                .await
                .map(WriteOutcome::Created)
                .map_err(classify)
        })
        .await
    }

    /// v1 codehub.rs 没有对应的 issue 状态查询函数(见模块文档)——如实
    /// `Unsupported`,不假装有。
    async fn issue_state(&self, cx: &CallCtx, _number: u32) -> ConnResult<IssueState> {
        unsupported(cx, Capability::IssueOps, "issue_state")
    }

    /// v1 codehub.rs 没有对应的关单函数(见模块文档)——如实 `Unsupported`。
    async fn close_issue(&self, cx: &CallCtx, _idem: IdemKey, _number: u32) -> ConnResult<()> {
        unsupported(cx, Capability::IssueOps, "close_issue")
    }

    /// design §5:`create_mr` → `IssueOps::open_change`。**一处诚实差异要
    /// 保住**(design §5 明文,主控裁决同):v1 注释明确 codehub 没有
    /// `Adopted` 路径——MR 已存在时 `mr create` 如实失败,这里原样
    /// `map_err(classify)`(多半落 `UpstreamRejected`),**不许**为了「跟
    /// github 长得一样」硬造一个 `AlreadyExisted`。
    ///
    /// 不过 v1 `create_mr` 内部其实也做了一次 `mr list --source-branch`
    /// 的存量 MR 探测(P7-7A parity 注释),命中时返回
    /// `crate::github::PrOpened::Adopted`(复用 github 的类型)——这不是
    /// 「造对称」,是 v1 原有逻辑,零改写照搬,这里的映射与 github 适配器
    /// 完全一致处理 `PrOpened::{Created,Adopted}`。真正「不对称」的地方是
    /// **`mr create` 打到 `already exists` 那句 stderr 时的行为**:github
    /// 的 `open_pr` 会再读一次 PR 号认领(`adopt_existing_pr`),codehub 的
    /// `create_mr` 没有这条兜底——如实继承,不补。
    async fn open_change(
        &self,
        cx: &CallCtx,
        _idem: IdemKey,
        req: OpenChangeReq,
    ) -> ConnResult<WriteOutcome<u32>> {
        let host = self.host().to_string();
        let path = self.path().to_string();
        guarded(cx, OpClass::Write, async move {
            up::create_mr(&host, &path, &req.workspace, req.issue_number, &req.title)
                .await
                .map(|opened| match opened {
                    crate::upstream::github::PrOpened::Created(n) => WriteOutcome::Created(n),
                    crate::upstream::github::PrOpened::Adopted(n) => {
                        WriteOutcome::AlreadyExisted(n)
                    }
                })
                .map_err(classify)
        })
        .await
    }

    /// v1 codehub.rs 没有独立的「查 MR 状态」函数(见模块文档)——如实
    /// `Unsupported`。`merge_mr`/`open_mr_for_branch` 各自内部会读状态,
    /// 但那是各自动作内部的私有校验,不是一个可独立调用的「查状态」能力。
    async fn change_state(&self, cx: &CallCtx, _number: u32) -> ConnResult<ChangeState> {
        unsupported(cx, Capability::IssueOps, "change_state")
    }

    /// design §5:`open_mr_for_branch` → `IssueOps::open_change_for_branch`。
    /// `Ok(None)` = 没有,不是错——上游行为,原样透传。
    async fn open_change_for_branch(&self, cx: &CallCtx, branch: &str) -> ConnResult<Option<u32>> {
        let host = self.host().to_string();
        let path = self.path().to_string();
        let branch = branch.to_string();
        guarded(cx, OpClass::Read, async move {
            up::open_mr_for_branch(&host, &path, &branch)
                .await
                .map_err(classify)
        })
        .await
    }

    /// design §5:`merge_mr` → `IssueOps::merge_change`。**人点的那一下**,
    /// 同 github 适配器。上游内部已经做了「读回为证」(merge 后复核
    /// state==merged 才算成功,退出码不可信),零改写照搬。
    async fn merge_change(&self, cx: &CallCtx, _idem: IdemKey, number: u32) -> ConnResult<()> {
        let host = self.host().to_string();
        let path = self.path().to_string();
        guarded(cx, OpClass::Write, async move {
            up::merge_mr(&host, &path, number).await.map_err(classify)
        })
        .await
    }

    // `checks` 未 override——v1 codehub.rs 没有对应实现(主控裁决 #3:切片
    // 二不做),吃基座 trait 的默认「如实 Unsupported」。
}
