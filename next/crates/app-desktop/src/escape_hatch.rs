//! 逃生舱(design-s5-hexpanel.md §5.2,主控裁决 7「本片必须真能用,评审
//! 要实测」):「进行中」的运行卡上显示真实上游会话号 + 可复制的「自己
//! 终端里续接」命令。**纯函数**——不碰网络/文件系统/时钟,只把已经从
//! `run` 表读回来的既成事实(`upstream_session`/`workspace`)格式化成一
//! 行命令,同 [`crate::kernel`] 模块文档「壳里禁推导」的边界:这里没有
//! 任何判断"这次运行是不是还活着"之类的业务逻辑,那是 `HexView.evidence.
//! runs`(六段视图现成的数据)按 `ended_at.is_none()` 筛出来的——筛选发
//! 生在调用方([`crate::kernel::build_vm`]),这个模块只管格式化。
//!
//! 三点如实(§5.2 原文):
//! - 命令语法是真的能跑通的——上游按工作目录编码存会话,`cd <这次运行
//!   的工作区路径> && claude --resume <会话号>` 缺一半都接不回去,所以
//!   两者必须一起显示,不能只给会话号;
//! - 会话号为空(这家 CLI 不能指派会话号,`run.upstream_session` 是空
//!   串)时,如实显示「这家不支持指派会话号,续接方式未知」,**不给一
//!   条编出来的命令**——`resume_command` 字段这时是 `None`,不是一个
//!   格式对但语义假的字符串;
//! - 这正是 plan/23 §3 三档接法里的第三档(「外开 + 事实回收」):不抓
//!   屏幕文本、不冒充已同步。

use bw_store::RunRow;

/// 一张逃生舱卡片的渲染就绪数据。
#[derive(Clone, Debug, PartialEq)]
pub struct EscapeHatch {
    /// 给人看的运行标签——运行编号前 8 位,不是完整 UUID(界面上够用,
    /// 完整编号仍然可以从 `run_label_full`/日志里查)。
    pub run_label: String,
    /// `None` = 这家连接器没能指派会话号。
    pub upstream_session: Option<String>,
    /// `None` = 没有会话号,续接方式未知,不给一条编出来的命令。
    pub resume_command: Option<String>,
}

/// 从一条运行行(通常是 `ended_at.is_none()` 的「进行中」运行)构建逃生
/// 舱卡片。
pub fn build(run: &RunRow) -> EscapeHatch {
    let full = run.id.uuid().to_string();
    let run_label = full.chars().take(8).collect::<String>();
    let session = run.upstream_session.trim();
    if session.is_empty() {
        EscapeHatch {
            run_label,
            upstream_session: None,
            resume_command: None,
        }
    } else {
        EscapeHatch {
            run_label,
            upstream_session: Some(session.to_string()),
            resume_command: Some(format!("cd {} && claude --resume {session}", run.workspace)),
        }
    }
}
