//! 健康永远推导 —— V4 版。
//!
//! V3 是「写透缓存」:算好的灯写进四个缓存列,界面读缓存。V4 改成**读时
//! 现算**:三条判据每次打开总览时从仓文件与 git 现场判,算完顺手把结果写回
//! `project` 的两个显示缓存列(那两列只服务项目墙——不打开项目也要能看见
//! 灯),总览自己从不拿它们当输入。
//!
//! 铁律的类型落点在 [`sealed`]:[`DerivedHealth`] 的构造函数是私有的,只有
//! 本模块的 [`derive_project_health`] 能造出一个来。`V4Store` 收灯的方法只
//! 认这个类型,所以「手工把项目点成绿」在类型上就不存在这条路。
//!
//! **没数据 = Unknown 灰,不是绿**。三条判据全不成立时给的是
//! [`Signal::Unknown`],不是「先当它是好的」。

mod sealed;

pub use sealed::DerivedHealth;

use crate::model::Signal;

/// 健康的三条判据。每条都能被独立复算(命令见
/// `docs/v4-prototype/design.md` 第 5 章「总览与健康」),不存任何中间值。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthInputs {
    /// (a) 本周有周目标(`.bw/plan/<本周>.md` 存在且「## 周目标」段非空)。
    pub has_week_goal: bool,
    /// (a) 本周仓里有真实 git 提交。
    pub committed_this_week: bool,
    /// 上周仓里有真实 git 提交。两周都没动是硬红。
    pub committed_last_week: bool,
    /// **git 到底读到没读到**。没配工作区、目录不是 git 仓、机器上没有 git,
    /// 这一条是假 —— 此时「两周零提交」说明不了任何事,不能当成「这个项目
    /// 停了」判红。没数据是灰,不是红,也不是绿。
    pub git_readable: bool,
    /// (b) 本周或上周的周计划文件里有「本周指标读数」段且非空。
    pub has_metric_reading: bool,
    /// 任何一条指标读数按 `.bw/metrics.toml` 的目标判下来是红。
    pub any_metric_red: bool,
    /// (c) 上周有合入(`git log --merges`)或发版(`.bw/releases.md` 新增行)。
    pub delivered_last_week: bool,
}

impl HealthInputs {
    /// 一个仓都还没有的项目:三条判据全假,灯是灰。
    pub fn empty() -> Self {
        Self {
            has_week_goal: false,
            committed_this_week: false,
            committed_last_week: false,
            git_readable: false,
            has_metric_reading: false,
            any_metric_red: false,
            delivered_last_week: false,
        }
    }

    fn on_track(&self) -> bool {
        self.has_week_goal && self.committed_this_week
    }
}

/// 一条给人看的理由。总览的 health 面板逐条列出来,不给一个孤零零的颜色。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthReason {
    pub ok: bool,
    pub text: String,
}

/// 唯一的灯来源。纯函数:同样的输入永远同样的输出,杀进程重开算得一样。
pub fn derive_project_health(inputs: &HealthInputs) -> DerivedHealth {
    let (a, b, c) = (
        inputs.on_track(),
        inputs.has_metric_reading,
        inputs.delivered_last_week,
    );

    // 判红在判绿之前:三条判据齐了但指标越线,那是红,不是绿。
    let stalled = inputs.git_readable && !inputs.committed_this_week && !inputs.committed_last_week;
    let signal = if inputs.any_metric_red || stalled {
        // 指标越线,或者读到了 git 而连着两周一条提交都没有 —— 后者不是
        // 「说不准」,是真的停了。
        Signal::Red
    } else if a && b && c {
        Signal::Green
    } else if !a && !b && !c {
        // 一条判据都不成立,又没有任何「确实停了」的证据:就是没数据。
        Signal::Unknown
    } else {
        Signal::Amber
    };

    let reasons = vec![
        HealthReason {
            ok: a,
            text: if a {
                "本周定了周目标,仓里也有真实提交".into()
            } else if !inputs.has_week_goal {
                "本周还没有周计划文件或周目标是空的".into()
            } else {
                "本周有周目标,但仓里还没有真实提交".into()
            },
        },
        HealthReason {
            ok: b,
            text: if b {
                "本周或上周的周计划文件里有指标读数".into()
            } else {
                "最近两周都没有记过指标读数".into()
            },
        },
        HealthReason {
            ok: c,
            text: if c {
                "上周有合入或发版".into()
            } else {
                "上周既没有合入也没有发版".into()
            },
        },
    ];

    sealed::mint(signal, reasons)
}
