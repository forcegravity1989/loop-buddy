//! 统一登记与分发:`ConnectorRegistry`。一条登记 = 种类 + 项目绑定身份 + 本机
//! 配置引用(`ConnectorEntry`,见 `contract.rs`)+ 它对应的活体适配器。注册表
//! 本身不知道 "gh" 这个词——是谁把 `GhConnector` 塞进来的,谁负责 feature 门
//! (design-s2-connector.md §2)。

use std::sync::Arc;

use bw_core::ProjectId;

use crate::caps::Connector;
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
    /// 两条校验:
    /// 1. **协议版本**(`debug_assert_eq!`):`conn.protocol() != PROTOCOL`
    ///    只可能是 composition root 装配代码忘了跟着契约改版本号——降级为
    ///    debug 断言,发布构建不为这类编码错误的兜底付运行时开销,调试构建
    ///    仍然 fail-fast。
    /// 2. **项目内 name 唯一性**(`panic!`,任何构建都查):同一项目下两条
    ///    连接器撞了 `name`,是 composition root 给它们起了同一个名字的
    ///    装配期编码错误,不是运行时可恢复状况,必须马上炸,不能留到查询时
    ///    才表现为「查出来的是另一条」这种更难查的坏味道。
    pub fn register(&mut self, entry: ConnectorEntry, conn: Arc<dyn Connector>) {
        debug_assert_eq!(
            conn.protocol(),
            PROTOCOL,
            "连接器 {:?} 声明协议版本 {} 与当前 {} 不符,拒绝登记(不做兼容层)",
            entry.kind,
            conn.protocol(),
            PROTOCOL
        );
        if let Some(dup) = self.items.iter().find(|r| {
            r.entry.binding.project == entry.binding.project && r.entry.name == entry.name
        }) {
            panic!(
                "项目 {:?} 内已有一条 name={:?} 的连接器登记(kind={:?}),\
                 新登记 kind={:?} 撞了同一个 name —— 这是 composition root \
                 的装配期编码错误(给两条连接器起了同一个名字),不是运行时可\
                 恢复状况",
                entry.binding.project, entry.name, dup.entry.kind, entry.kind
            );
        }
        self.items.push(Registered { entry, conn });
    }

    /// 全量列举(界面「连接器」区、探活巡检用)。
    pub fn entries(&self) -> impl Iterator<Item = &ConnectorEntry> {
        self.items.iter().map(|r| &r.entry)
    }

    /// 按项目 + 探活能力路由。返回**持有型**结果(entry 克隆、`Arc` 克隆),
    /// 不是借用切片:并发探活/巡检要把每一条连接器各自 `tokio::spawn` 出去,
    /// spawn 出去的 future 需要 `'static`,借用 `&self` 做不到。调用点拿到
    /// `Arc<dyn Connector>` 后自己 `.as_probe()` 上转成 `&dyn Probe`(上转出来
    /// 的引用借的是这份 `Arc` 自己的生命周期,不再借注册表)。
    ///
    /// 一个项目可有多条连接器都支持探活(仓连接器 + 采集脚本各自能探),天然
    /// 多条,不做「取第一条」。
    pub fn probes(&self, p: ProjectId) -> Vec<(ConnectorEntry, Arc<dyn Connector>)> {
        self.items
            .iter()
            .filter(|r| r.entry.binding.project == p && r.conn.as_probe().is_some())
            .map(|r| (r.entry.clone(), r.conn.clone()))
            .collect()
    }

    /// 按项目 + 采集能力路由。一个项目可有多个采集脚本,天然多条。持有型
    /// 返回,理由同 [`ConnectorRegistry::probes`]。
    pub fn collectors(&self, p: ProjectId) -> Vec<(ConnectorEntry, Arc<dyn Connector>)> {
        self.items
            .iter()
            .filter(|r| r.entry.binding.project == p && r.conn.as_collect().is_some())
            .map(|r| (r.entry.clone(), r.conn.clone()))
            .collect()
    }

    /// 按项目 + 执行能力路由。持有型返回,理由同
    /// [`ConnectorRegistry::probes`]。
    pub fn executors(&self, p: ProjectId) -> Vec<(ConnectorEntry, Arc<dyn Connector>)> {
        self.items
            .iter()
            .filter(|r| r.entry.binding.project == p && r.conn.as_execute().is_some())
            .map(|r| (r.entry.clone(), r.conn.clone()))
            .collect()
    }

    /// 仓连接器是**每项目至多一条**(一个项目的活提到哪个仓,不能有歧义)。
    /// 找到零条 → `Err(NotConnected)`;找到多条 → `Err(Ambiguous)`,绝不
    /// 「取第一条」蒙混。持有型返回,理由同 [`ConnectorRegistry::probes`]
    /// (issue_ops 也会被巡检/飘移检查并发调用)。
    pub fn issue_ops(
        &self,
        p: ProjectId,
    ) -> Result<(ConnectorEntry, Arc<dyn Connector>), RoutingError> {
        let mut matches: Vec<(ConnectorEntry, Arc<dyn Connector>)> = self
            .items
            .iter()
            .filter(|r| r.entry.binding.project == p && r.conn.as_issue_ops().is_some())
            .map(|r| (r.entry.clone(), r.conn.clone()))
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
