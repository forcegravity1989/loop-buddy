//! 命令用例,按聚合拆(design-s5-hexpanel.md §4.2)——项目/活/指标各自一
//! 片,不是一个巨型分发器堆着所有用例正文。[`crate::lib`] 的
//! [`crate::Command`]/[`crate::App::dispatch`] 只做路由,真正的用例逻辑
//! 全部住在这三个子模块里。

pub mod issue;
pub mod metric;
pub mod project;
