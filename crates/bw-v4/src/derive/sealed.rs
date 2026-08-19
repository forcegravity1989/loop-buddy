//! 密封件:`DerivedHealth` 只能在 `crate::derive` 内部造出来。
//!
//! 和 `bw-core` 的 `Derived<Signal>` 同一个把戏、同一个理由——把「信号只能
//! 从数据推导」从一条纪律变成一条类型规则。区别只是判据不同:V3 是指标
//! 观测链,V4 是仓文件与 git 的三条现算判据。

use super::HealthReason;
use crate::model::Signal;

/// 一个推导出来的健康结论。没有 `pub fn new`——外部拿到它的唯一途径是调用
/// [`super::derive_project_health`]。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedHealth {
    signal: Signal,
    reasons: Vec<HealthReason>,
}

/// 唯一的铸造点,`pub(super)` ⇒ 只有 `crate::derive` 调得到。
pub(super) fn mint(signal: Signal, reasons: Vec<HealthReason>) -> DerivedHealth {
    DerivedHealth { signal, reasons }
}

impl DerivedHealth {
    pub fn signal(&self) -> Signal {
        self.signal
    }

    /// 项目墙那一格的灯。V3 的 `weekly_signal` 从来和 `signal` 写成同一个值
    /// (读代码可见),V4 不假装它是别的东西——三条判据本来就是按周窗口算
    /// 的,这里如实返回同一个值,不另造一个看起来更细的假区分。
    pub fn weekly_signal(&self) -> Signal {
        self.signal
    }

    pub fn reasons(&self) -> &[HealthReason] {
        &self.reasons
    }
}
