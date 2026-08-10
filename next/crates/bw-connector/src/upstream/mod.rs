//! 上游 v1 代码整体移植区。**这里的东西不归你改**——要改去上游 CLI(`gh`/
//! `codehub-cli`)本身,或者去 `adapters/` 层包一层(design-s2-connector.md
//! §1)。函数体一个字都没动;唯一动过的是 `use` 路径(crate 结构变了)。
//!
//! `github.rs`/`codehub.rs` 是 v1 `crates/bw-engine/src/{github,codehub}.rs`
//! 的整体搬迁(逐字节内容不变,只改了 `use crate::workspace::…` 这一类路径)。
//! `workspace.rs` **不是**整体搬迁——它只收了被这两个文件真实引用到的四项
//! (`git_in`/`commit_initial`/`stage_commit_push`/`ProvisionError`),v1
//! 原文件里另外七项(`provision_git_workspace`/`commit_file`/`write_file`/
//! `IssueWorktreeGuard`/`provision_issue_worktree`/`is_owned_workspace`/
//! `FileChange`+`diff_numstat`)没被引用,没有搬(主控裁决 #1:workspace 辅助
//! 落这里,单副本;移植清单见 next 切片二B 的 commit 正文)。

#[cfg(any(feature = "gh", feature = "codehub"))]
pub mod workspace;

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
