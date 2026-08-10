//! agentcli 层(next 切片三,design-s3-agentcli.md §1 落点定案)。把已经移植
//! 进 `bw-engine` 的终端栈(`interactive_cli`/`terminal_manager`/
//! `pty_backend`)接上切片二定型的 `bw_connector::caps::Execute` 接口,让
//! claude 成为第一个真正能跑的执行连接器。
//!
//! **落点**:装在 `bw-engine` 里,不新开 crate——PTY 依赖(`portable-pty`/
//! `conpty-oxide`)已经在这里,新开 crate 只多一份 manifest,不多一分隔离
//! (design §1.1 三条理由)。
//!
//! **两个子模块,对应设计稿两张表**(§1.2「两张表,一个适配器持有」):
//! - [`session`](切片三C):会话身份的内存表——`SessionRow`/`SessionTable`/
//!   `InjectRecord`,加一个上游目录反查 `discover_sessions`(§2.1)。**不落
//!   任何 BW 自有持久化**——上游(`~/.claude/projects/…`)已经是正本,BW 再
//!   存一份就是开第二本账;切片四/五建活的存储时才轮到这张表的落盘版
//!   (`CLAUDE.md`「不为向后兼容留旧路径」/design §2.1)。
//! - [`connector`](切片三D):`AgentCliConnector`——`bw_connector::caps::
//!   Connector` + `Probe` + `Execute` 的真实实现(claude 一家)。把
//!   `session` 的会话表、`interactive_cli` 的计划构造器、`pty_backend` 的
//!   平台后端接起来;持有 `terminal_manager::TerminalManager`(字节路由,
//!   界面面向)与 `session::SessionTable`(生命周期,编排层面向)两张表,
//!   键不同,不绑在一起(§1.2)。
//!
//! **必须保住的一条性质**:编排层(切片四建的 `bw-app`)只依赖
//! `bw-connector`,不依赖 `bw-engine`——它拿到的是注册表里的
//! `Arc<dyn Connector>`,谁把 `AgentCliConnector` 塞进去是组装处(桌面壳/
//! 指挥器)的事。这条边界靠依赖方向天然守住(`bw-app` 的 manifest 里没有
//! `bw-engine` 才编得过),不需要额外守卫脚本。
//!
//! **顺带记一条容易被误判的事**:`scripts/guard-no-direct-process.sh` 只查
//! `bw-core`/`bw-app` 两个目录,`bw-engine`(含这里)不在其中——agentcli 层
//! 起 PTY 子进程是设计如此(它就是那个「对外能力」的实现方),不是绕过
//! 守卫。

pub mod connector;
pub mod session;

pub use connector::{assemble_system_prompt, AgentCliConnector};
pub use session::{discover_sessions, InjectRecord, SessionRow, SessionTable};
