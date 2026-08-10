//! 统一登记与分发:`ConnectorRegistry`。一条登记 = 种类 + 项目绑定身份 + 本机
//! 配置引用(`ConnectorEntry`,见 `contract.rs`)+ 它对应的活体适配器。注册表
//! 本身不知道 "gh" 这个词——是谁把 `GhConnector` 塞进来的,谁负责 feature 门
//! (design-s2-connector.md §2)。

use std::sync::Arc;

use bw_core::ProjectId;

use crate::caps::{Collect, Connector, Execute, IssueOps, Probe};
use crate::contract::{Capability, ConnectorEntry, PROTOCOL};

/// 一条登记 + 它对应的活体适配器。
struct Registered {
    entry: ConnectorEntry,
    conn: Arc<dyn Connector>,
}

/// 项目 × 能力路由表。装载来源见 design §2「装载来源」表:GithubRepo/
/// CodehubRepo 来自项目行的 provider/remote_host/remote_path,Script 来自
/// `.bw/connectors.toml`,AgentCli 来自 agentcli 层的静态注册表(切片三)。
#[derive(Default)]
pub struct ConnectorRegistry {
    items: Vec<Registered>,
}

impl ConnectorRegistry {
    /// 装载入口。composition root(桌面壳 / headless 指挥器)调它。
    ///
    /// 注册时校验协议版本:`conn.protocol() != PROTOCOL` 一律拒绝登记(而不是
    /// 运行时才炸)——协议不匹配只能是composition root 装配错误(忘了跟着契约
    /// 改版本),不是可恢复的运行时状况,fail-fast 用 panic。
    pub fn register(&mut self, entry: ConnectorEntry, conn: Arc<dyn Connector>) {
        assert_eq!(
            conn.protocol(),
            PROTOCOL,
            "连接器 {:?} 声明协议版本 {} 与当前 {} 不符,拒绝登记(不做兼容层)",
            entry.kind,
            conn.protocol(),
            PROTOCOL
        );
        self.items.push(Registered { entry, conn });
    }

    /// 全量列举(界面「连接器」区、探活巡检用)。
    pub fn entries(&self) -> impl Iterator<Item = &ConnectorEntry> {
        self.items.iter().map(|r| &r.entry)
    }

    /// 按项目 + 探活能力路由。返回**引用切片**,不是单个:采集连接器天然
    /// 多条,探活也一样(一个项目的每条连接器都可能各自被探)。
    pub fn probes(&self, p: ProjectId) -> Vec<(&ConnectorEntry, &dyn Probe)> {
        self.items
            .iter()
            .filter(|r| r.entry.binding.project == p)
            .filter_map(|r| r.conn.as_probe().map(|c| (&r.entry, c)))
            .collect()
    }

    /// 按项目 + 采集能力路由。一个项目可有多个采集脚本,天然多条。
    pub fn collectors(&self, p: ProjectId) -> Vec<(&ConnectorEntry, &dyn Collect)> {
        self.items
            .iter()
            .filter(|r| r.entry.binding.project == p)
            .filter_map(|r| r.conn.as_collect().map(|c| (&r.entry, c)))
            .collect()
    }

    /// 按项目 + 执行能力路由。
    pub fn executors(&self, p: ProjectId) -> Vec<(&ConnectorEntry, &dyn Execute)> {
        self.items
            .iter()
            .filter(|r| r.entry.binding.project == p)
            .filter_map(|r| r.conn.as_execute().map(|c| (&r.entry, c)))
            .collect()
    }

    /// 仓连接器是**每项目至多一条**(一个项目的活提到哪个仓,不能有歧义)。
    /// 找到零条 → `Err(NotConnected)`;找到多条 → `Err(Ambiguous)`,绝不
    /// 「取第一条」蒙混。
    pub fn issue_ops(
        &self,
        p: ProjectId,
    ) -> Result<(&ConnectorEntry, &dyn IssueOps), RoutingError> {
        let mut matches: Vec<(&ConnectorEntry, &dyn IssueOps)> = self
            .items
            .iter()
            .filter(|r| r.entry.binding.project == p)
            .filter_map(|r| r.conn.as_issue_ops().map(|c| (&r.entry, c)))
            .collect();
        match matches.len() {
            0 => Err(RoutingError::NotConnected(Capability::IssueOps)),
            1 => Ok(matches.remove(0)),
            n => Err(RoutingError::Ambiguous {
                cap: Capability::IssueOps,
                n,
            }),
        }
    }
}

/// 仓连接器路由失败的两种如实情形——绝不「取第一条」蒙混过去。
#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("项目未绑定{0}能力的连接器")]
    NotConnected(Capability),
    #[error("项目绑定了 {n} 条{cap}连接器,无法判定用哪条")]
    Ambiguous { cap: Capability, n: usize },
}
