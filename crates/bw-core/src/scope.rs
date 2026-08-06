//! plan/20 R2 · 作用域就近解析(most specific wins)。
//!
//! 三层隔离(全局基础库 / 项目资产 / 工作目录)里,一切**按名解析**的路径
//! (技能注入、cron 按名找 workflow、记账行定位)共用这一条规则:
//!
//! - 本项目行 > 全局行(`project_id IS NULL`);
//! - **他项目的行永不命中**——跨项目绝不泄漏、绝不污账;
//! - 就近优先是**行级遮蔽**:项目行存在即遮蔽全局同名行,哪怕项目行
//!   content 为空(可用性检查是调用方注入前的事)。遮蔽行确定,记账行
//!   才能与注入行严格同一(R3:记账行 == 注入行);
//! - 同一作用域内撞名(外库导入可致)按输入的既有稳定序取首行——如实,
//!   不猜。
//!
//! 纯函数、零 IO,wasm32 可编译——规则钉在内核,bw-app 与 UI 共用,
//! 不各写一份漂移。

use crate::ids::ProjectId;

/// 在 `rows` 里找满足 `matches` 的行,按就近优先归位:`project` 语境下
/// 项目行胜出,否则全局行兜底;`project = None`(全局语境)只命中全局行。
/// 他项目的行在任何语境下都不命中。
pub fn scoped_pick<'a, T>(
    rows: impl IntoIterator<Item = &'a T>,
    project: Option<ProjectId>,
    owner_of: impl Fn(&T) -> Option<ProjectId>,
    matches: impl Fn(&T) -> bool,
) -> Option<&'a T> {
    let mut global_hit: Option<&'a T> = None;
    for r in rows {
        if !matches(r) {
            continue;
        }
        match owner_of(r) {
            Some(owner) if Some(owner) == project => return Some(r),
            None if global_hit.is_none() => global_hit = Some(r),
            _ => {}
        }
    }
    global_hit
}

/// plan/20 R1 · 选择池成员资格:`owner` 为全局或本项目即可见;他项目的行
/// 在任何池里都不出现。`project = None`(全局语境的表单)时只剩全局行——
/// 引用的作用域不得宽于引用者。
pub fn in_scope(owner: Option<ProjectId>, project: Option<ProjectId>) -> bool {
    match owner {
        None => true,
        Some(o) => Some(o) == project,
    }
}
