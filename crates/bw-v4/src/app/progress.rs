//! 长命令的「一步一句」回执。
//!
//! 接一个项目要 clone 仓、读名片、写文件,每一步都是几秒。等整条命令做完才
//! 回一句总结,人点完那一下界面纹丝不动 —— 只会以为没点上,然后猛点。所以
//! 长命令边做边报:开一步报一句「正在……」,做完把同一句原地改写成结果。
//!
//! 这条通道**只负责让人看得见,不承担任何账目**。真话仍然只以命令回的
//! [`Event`](crate::command::Event)、库里的行、仓里的文件为准 —— 这里报了
//! 「clone 好了」而仓不在,以仓为准。没人订阅就静静丢掉,绝不因为报不出去
//! 让命令失败。

/// 一步的结局。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    /// 正在做。界面上转圈。
    Doing,
    Ok,
    /// 这一步没成。**不代表整条命令失败** —— 失败与否看命令自己返回什么。
    Fail,
}

/// 一行回执。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressLine {
    /// 第几步。界面按这个号**原地覆盖**同号的旧行 —— 「正在 clone…」被
    /// 「clone 好了」换掉,而不是堆成两行。
    pub step: u8,
    pub state: StepState,
    pub text: String,
}

impl ProgressLine {
    pub fn doing(step: u8, text: impl Into<String>) -> Self {
        Self {
            step,
            state: StepState::Doing,
            text: text.into(),
        }
    }
    pub fn ok(step: u8, text: impl Into<String>) -> Self {
        Self {
            step,
            state: StepState::Ok,
            text: text.into(),
        }
    }
    pub fn fail(step: u8, text: impl Into<String>) -> Self {
        Self {
            step,
            state: StepState::Fail,
            text: text.into(),
        }
    }
}
