//! `bw-connector` —— BW 对外借力的连接器契约层(next 切片二A)。
//!
//! 三层结构:`contract`(最小机器契约类型——协议版本、能力名、请求/防重编号、
//! 写操作结果、七档错误分类、调用上下文、连接器登记身份三件套)、`caps`
//! (按能力拆的四个小接口 `Probe`/`Execute`/`Collect`/`IssueOps` + 基座 trait
//! `Connector`)、`registry`(项目 × 能力路由的登记表 `ConnectorRegistry`)。
//!
//! `adapters/` 下每家适配器各自一个 feature 门(`gh` / `codehub` / `script`)。
//! **骨架阶段只立 feature 边界与占位模块,不写任何具体适配器实现**——
//! gh/codehub/script 三家的真实收编(整体搬 v1 `github.rs` / `codehub.rs` /
//! `connectors_file.rs`)是下一任务(切片二B)。
//!
//! 依赖方向:`bw-connector → bw-core`,单向,不依赖 `bw-engine`。

pub mod adapters;
pub mod caps;
pub mod contract;
pub mod registry;

pub use caps::{ChangeState, CheckConclusion, CheckRun, Collect, CollectOut, CollectReq};
pub use caps::{Connector, Execute, IssueOps, IssueState, OpenChangeReq, Probe, ProbeReport};
pub use contract::{
    guarded, CallCtx, CallOk, Capability, CapabilitySet, ConfigRef, ConnError, ConnResult,
    ConnectorEntry, ConnectorKind, ExecSpec, ExecState, ExecTicket, Fail, IdemKey, InjectBlock,
    OpClass, ProjectBinding, RequestId, WriteOutcome, PROTOCOL,
};
pub use registry::{ConnectorRegistry, RoutingError};
