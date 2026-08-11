//! `bw-connector` —— BW 对外借力的连接器契约层(next 切片二A 地基 + 切片二B
//! gh/codehub 收编 + 切片二C script 收编 + `probe_all` 行为验收指挥器)。
//!
//! 三层结构:`contract`(最小机器契约类型——协议版本、能力名、请求/防重编号、
//! 写操作结果、七档错误分类、调用上下文、连接器登记身份三件套)、`caps`
//! (按能力拆的四个小接口 `Probe`/`Execute`/`Collect`/`IssueOps` + 基座 trait
//! `Connector`)、`registry`(项目 × 能力路由的登记表 `ConnectorRegistry`)。
//!
//! `adapters/` 下每家适配器各自一个 feature 门(`gh` / `codehub` / `script`)。
//! `gh`/`codehub` 两家在切片二B 真实收编(整体搬 v1 `github.rs`/
//! `codehub.rs`,函数体零改写,见 `upstream`);`script`(`.bw/connectors.toml`
//! 脚本采集)在切片二C 真实收编(新写,`adapters::script` 是适配器,
//! `adapters::script_source` 是正本解析器,主控裁决 #9)。
//!
//! `upstream/` 是搬过来的 v1 原文,除 `use` 路径修正外零改写——「这里面的
//! 东西不归你改,要改去上游 CLI 或去 `adapters/` 层」(design §1)。
//!
//! 依赖方向:`bw-connector → bw-core`,单向,不依赖 `bw-engine`。next 切片
//! 五A 起还有 `bw-connector → bw-workspace`(`gh`/`codehub` 两个 feature 门
//! 下,复用换落点过去的三个 git 辅助——design-s5-hexpanel.md §6.2)。

pub mod adapters;
pub mod caps;
pub mod contract;
pub mod registry;
pub mod upstream;

#[cfg(feature = "codehub")]
pub use adapters::codehub::CodehubConnector;
pub use adapters::from_entry;
#[cfg(feature = "gh")]
pub use adapters::gh::GhConnector;
#[cfg(feature = "script")]
pub use adapters::script::ScriptConnector;
#[cfg(feature = "script")]
pub use adapters::script_source::{self, ScriptSourceError, CONNECTORS_FILE_REL_PATH};
pub use caps::{ChangeState, CheckConclusion, CheckRun, Collect, CollectOut, CollectReq};
pub use caps::{Connector, Execute, Interactive, IssueOps, IssueState, OpenChangeReq};
pub use caps::{Probe, ProbeReport, TermInput};
pub use contract::{
    guarded, unsupported, CallCtx, CallOk, Capability, CapabilitySet, ConfigRef, ConnError,
    ConnResult, ConnectorEntry, ConnectorKind, ExecSpec, ExecState, ExecTicket, Fail, IdemKey,
    InjectBlock, OpClass, ProjectBinding, RequestId, SessionEnd, StopReason, WriteOutcome,
    PROTOCOL, TASK_BODY_LABEL,
};
pub use registry::{ConnectorRegistry, RoutingError};
