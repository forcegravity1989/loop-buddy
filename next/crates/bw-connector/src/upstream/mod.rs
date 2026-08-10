//! 上游 v1 代码整体移植区。**这里的东西不归你改**——要改去上游 CLI(`gh`/
//! `codehub-cli`)本身,或者去 `adapters/` 层包一层(design-s2-connector.md
//! §1)。函数体一个字都没动;唯一动过的是 `use` 路径(crate 结构变了)。
//!
//! `github.rs`/`codehub.rs` 是 v1 `crates/bw-engine/src/{github,codehub}.rs`
//! 的整体搬迁(逐字节内容不变,只改了 `use crate::workspace::…` 这一类路径)。
//!
//! **next 切片五A**:曾经住在这里的 `workspace.rs`(`git_in`/
//! `commit_initial`/`stage_commit_push`/`ProvisionError` 四项,v1 原文件
//! 十一项里被 `github.rs`/`codehub.rs` 真实引用到的子集,切片二B 收编)已
//! **移**到 `bw-workspace` crate 去(design-s5-hexpanel.md §6.2/§9:它们不
//! 是「对外连接」,本地 git 读写是内建工作区函数,不该住在连接器 crate 里
//! 当「内建函数」——那是切片二裁决 #1 留下的命名将就,切片三开放问题 4
//! 已经登记过)。`github.rs`/`codehub.rs` 的 `use` 路径已改指向
//! `bw_workspace::{…}`;`bw-connector` 因此反过来依赖 `bw-workspace`(见
//! `Cargo.toml`,`gh`/`codehub` 两个 feature 门下)。单副本,不复制——这
//! 里不再有一份自己的拷贝。

// v1 `codehub.rs` 的 `create_mr` 直接复用 `github::PrOpened`(其自身 doc
// comment 明说是「P7-7A parity」——created-vs-adopted 这套判定是两家共用的
// 一个概念,v1 从没把它拆成独立类型)。这是收编时才发现的真实结构耦合,不是
// 本次移植引入的:v1 里 github.rs/codehub.rs 从来都是一起编译的,没有互相
// 独立的 feature 边界。`PrOpened` 是引用类型(未被引用的项才允许删,见
// `workspace.rs` 头注释),不能从 github.rs 里挪走或复制一份改名——那都会
// 越过「只许改 use 路径」的移植纪律。因此这里放宽 `github` 模块本身的
// 编译门槛(不是 `gh` 适配器/连接器的门槛,那个仍在 `adapters/mod.rs`
// 严格锁 `feature = "gh"`):`codehub` 单开时也编译 `upstream::github`(它的
// 函数都是 `pub` 自由函数,没被挂进任何连接器接口,不会被当成「gh 连接器
// 被拖带进来了」)。方向上只有 codehub → github 这一条(github.rs 反过来
// 不引用 codehub 任何东西,`--features gh` 单开不受影响)。
#[cfg(any(feature = "gh", feature = "codehub"))]
pub mod github;

#[cfg(feature = "codehub")]
pub mod codehub;
