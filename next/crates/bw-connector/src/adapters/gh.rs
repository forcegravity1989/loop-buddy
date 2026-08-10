//! gh(GitHub CLI)连接器适配器(feature = "gh")。整体收编 v1
//! `crates/bw-engine/src/github.rs`(见 `crate::upstream::github`,函数体零
//! 改写)——这里只是薄包装:trait 实现 + [`guarded`] 接线 + 错误/状态归一化。
//! design-s2-connector.md §5「收编映射:v1 github.rs(gh)」逐条对照。

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
use crate::upstream::github as up;

/// gh 连接器。`binding.host` 恒 `"github.com"`(gh 走全局登录,host 字段不用
/// 但保留三家对称);`binding.path` = `owner/repo`。
pub struct GhConnector {
    kind: ConnectorKind,
    binding: ProjectBinding,
}

impl GhConnector {
    pub fn new(binding: ProjectBinding) -> Self {
        Self {
            kind: ConnectorKind::GithubRepo,
            binding,
        }
    }

    /// 登记工厂用的构造入口(brief 要求:「两家的构造函数(from
    /// ConnectorEntry)接入注册表工厂」)——composition root 按 `entry.kind`
    /// 分派到这里(见 `adapters::from_entry`)。这里再校验一次 kind 对不对:
    /// 校验失败只可能是装配期编码错误(拿一条 `CodehubRepo` 登记硬塞给
    /// `GhConnector`),不是运行时可恢复状况,`panic!`——同
    /// `ConnectorRegistry::register` 对撞名的处理口径。
    pub fn from_entry(entry: &ConnectorEntry) -> Arc<dyn Connector> {
        assert_eq!(
            entry.kind,
            ConnectorKind::GithubRepo,
            "GhConnector::from_entry 收到非 GithubRepo 登记(name={:?},kind={:?})——\
             composition root 的装配期编码错误",
            entry.name,
            entry.kind
        );
        Arc::new(Self::new(entry.binding.clone()))
    }

    fn owner_repo(&self) -> &str {
        &self.binding.path
    }
}

impl Connector for GhConnector {
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

/// `GithubError` → 七档结构化分类。集中在这一处(主控裁决 #6)。
///
/// **本适配器目前只落一条映射规则**,且是结构性的、不是字符串嗅探:
/// `GithubError::NotInstalled` 来自 `spawn_err` 对 `std::io::ErrorKind::
/// NotFound` 的判定(上游代码里的精确匹配,不是猜),直接对应
/// `ConnError::NotConnected`,不需要验证日期。
///
/// `GithubError::Command(String)` 包着 gh 的原始 stderr(也包着几处上游自己
/// 拼的中文解析失败提示,比如"无法从 gh 输出解析 issue 号")——2026-08-10
/// 尝试在本机现场核验"未登录"/"仓不存在"等 stderr 文案时,`gh repo view`
/// (陌生仓只读探测)与 `gh issue create`(会真实写入他人仓库)均被沙箱权限
/// 分类器拦下,无法安全拿到可验证的真实文案。按裁决 #6「映射不到的落
/// `UpstreamRejected` 原文透传」处理,不落未经验证的字符串规则——诚实优先
/// 于覆盖率。
fn classify(err: up::GithubError) -> ConnError {
    match err {
        up::GithubError::NotInstalled => ConnError::NotConnected("gh 未安装或不在 PATH".into()),
        up::GithubError::Command(msg) => ConnError::UpstreamRejected { message: msg },
    }
}

fn normalize_issue_state(raw: &str) -> Result<IssueState, ConnError> {
    match raw {
        "OPEN" => Ok(IssueState::Open),
        "CLOSED" => Ok(IssueState::Closed),
        other => Err(ConnError::Unparsable {
            raw: other.to_string(),
        }),
    }
}

fn normalize_change_state(raw: &str) -> Result<ChangeState, ConnError> {
    match raw {
        "OPEN" => Ok(ChangeState::Open),
        "MERGED" => Ok(ChangeState::Merged),
        "CLOSED" => Ok(ChangeState::Closed),
        other => Err(ConnError::Unparsable {
            raw: other.to_string(),
        }),
    }
}

#[async_trait]
impl Probe for GhConnector {
    /// design §5:`probe_repo` → `Probe::probe`,一行人话直接进
    /// `ProbeReport.detail`。真的跑一次 `gh repo view`——「装了≠连上了」,
    /// 探不通就 `Err`,绝不因为 gh 装了就报成功。
    async fn probe(&self, cx: &CallCtx) -> ConnResult<ProbeReport> {
        let owner_repo = self.owner_repo().to_string();
        let identity = owner_repo.clone();
        guarded(cx, OpClass::Probe, async move {
            up::probe_repo(&owner_repo)
                .await
                .map(|detail| ProbeReport { identity, detail })
                .map_err(classify)
        })
        .await
    }
}

#[async_trait]
impl Collect for GhConnector {
    /// design §5:`collect_github_count`(+ `expand_query`/`days_ago_iso`,
    /// 滚动窗口宏在上游内部展开,不进契约)→ `Collect::collect(RemoteCount)`。
    /// gh 不做脚本采集——`ScriptRun` 如实 `Unsupported`,不给假空 JSON。
    async fn collect(&self, cx: &CallCtx, req: CollectReq) -> ConnResult<CollectOut> {
        match req {
            CollectReq::ScriptRun => unsupported(cx, Capability::Collect, "script_run"),
            CollectReq::RemoteCount { query, today } => {
                let owner_repo = self.owner_repo().to_string();
                guarded(cx, OpClass::CollectCount, async move {
                    up::collect_github_count(&owner_repo, &query, today)
                        .await
                        .map(|n| CollectOut {
                            value: serde_json::Value::from(n),
                            source_hint: format!("gh search/issues · {owner_repo} · {query}"),
                        })
                        .map_err(classify)
                })
                .await
            }
        }
    }
}

#[async_trait]
impl IssueOps for GhConnector {
    /// design §5:`create_issue` → `IssueOps::create_issue`。上游没有
    /// read-before-write(按标题查重不可靠),`WriteOutcome` 恒
    /// `Created`——如实标注的不对称(主控裁决 #4,`idem` 只作日志追溯)。
    async fn create_issue(
        &self,
        cx: &CallCtx,
        _idem: IdemKey,
        title: &str,
        body: &str,
    ) -> ConnResult<WriteOutcome<u32>> {
        let owner_repo = self.owner_repo().to_string();
        let title = title.to_string();
        let body = body.to_string();
        guarded(cx, OpClass::Write, async move {
            up::create_issue(&owner_repo, &title, &body)
                .await
                .map(WriteOutcome::Created)
                .map_err(classify)
        })
        .await
    }

    /// design §5:`issue_state` → `IssueOps::issue_state`,`OPEN`/`CLOSED`
    /// 归一化到 `IssueState`。
    async fn issue_state(&self, cx: &CallCtx, number: u32) -> ConnResult<IssueState> {
        let owner_repo = self.owner_repo().to_string();
        guarded(cx, OpClass::Read, async move {
            let raw = up::issue_state(&owner_repo, number)
                .await
                .map_err(classify)?;
            normalize_issue_state(&raw)
        })
        .await
    }

    /// design §5:`close_issue` → `IssueOps::close_issue`。已关再关是
    /// no-op 成功,天然幂等(上游行为,未改写)。
    async fn close_issue(&self, cx: &CallCtx, _idem: IdemKey, number: u32) -> ConnResult<()> {
        let owner_repo = self.owner_repo().to_string();
        guarded(cx, OpClass::Write, async move {
            up::close_issue(&owner_repo, number).await.map_err(classify)
        })
        .await
    }

    /// design §5:`open_pr`(+ `adopt_existing_pr`)→ `IssueOps::open_change`。
    /// `PrOpened::{Created,Adopted}` 原样映射到
    /// `WriteOutcome::{Created,AlreadyExisted}`——契约里 `WriteOutcome` 的
    /// 设计来源,这条 read-before-write(按 head 分支查已开 PR)是上游
    /// `open_pr` 内部**自己**做的,不是这里另起的逻辑。`idem` 未被上游函数
    /// 消费——真正的防重锚点是 head 分支,这个参数收着是为将来运行管理器的
    /// 审计追溯,不代表这里凭它做判断。
    async fn open_change(
        &self,
        cx: &CallCtx,
        _idem: IdemKey,
        req: OpenChangeReq,
    ) -> ConnResult<WriteOutcome<u32>> {
        guarded(cx, OpClass::Write, async move {
            up::open_pr(&req.workspace, req.issue_number, &req.title)
                .await
                .map(|opened| match opened {
                    up::PrOpened::Created(n) => WriteOutcome::Created(n),
                    up::PrOpened::Adopted(n) => WriteOutcome::AlreadyExisted(n),
                })
                .map_err(classify)
        })
        .await
    }

    /// design §5:`pr_state` → `IssueOps::change_state`,`OPEN`/`MERGED`/
    /// `CLOSED` 归一化到 `ChangeState`。
    async fn change_state(&self, cx: &CallCtx, number: u32) -> ConnResult<ChangeState> {
        let owner_repo = self.owner_repo().to_string();
        guarded(cx, OpClass::Read, async move {
            let raw = up::pr_state(&owner_repo, number).await.map_err(classify)?;
            normalize_change_state(&raw)
        })
        .await
    }

    /// design §5:`open_pr_for_branch` → `IssueOps::open_change_for_branch`。
    /// `Ok(None)` = 没有,不是错——上游行为,原样透传。
    async fn open_change_for_branch(&self, cx: &CallCtx, branch: &str) -> ConnResult<Option<u32>> {
        let owner_repo = self.owner_repo().to_string();
        let branch = branch.to_string();
        guarded(cx, OpClass::Read, async move {
            up::open_pr_for_branch(&owner_repo, &branch)
                .await
                .map_err(classify)
        })
        .await
    }

    /// design §5:`merge_pr` → `IssueOps::merge_change`。**人点的那一下**——
    /// 只能由编排层的显式合入命令调,任何执行路径不许调(这条纪律靠调用方
    /// 遵守,连接器这一层管不了谁调它)。
    async fn merge_change(&self, cx: &CallCtx, _idem: IdemKey, number: u32) -> ConnResult<()> {
        let owner_repo = self.owner_repo().to_string();
        guarded(cx, OpClass::Write, async move {
            up::merge_pr(&owner_repo, number).await.map_err(classify)
        })
        .await
    }

    // `checks` 未 override——v1 github.rs 没有对应实现(主控裁决 #3:切片二
    // 不做),吃基座 trait 的默认「如实 Unsupported」。
}
