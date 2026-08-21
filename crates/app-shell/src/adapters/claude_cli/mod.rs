//! Claude CLI 开工工具的适配模块。
//!
//! 声明这个工具是什么接法、要什么、能干什么;真正起进程的是内核那边的交互式
//! 执行器,这里不重复实现一遍。

/// 接法类型。终端类 = 起一个 PTY 子进程;本机网页内嵌类 = 探到端口后嵌 WebView。
/// 今天只有一个终端类工具,所以还没有按 `KIND` 分发的调用方 —— 加第二个工具
/// 那天它才有用。
#[allow(dead_code)]
pub const KIND: &str = "terminal";

/// 这个工具需要什么才能用。安装期把这几条摆给人看,不要等跑起来才报错。
pub const REQUIRES: &[&str] = &["本机已装 claude", "一个真实的工作区路径", "能连上模型网关"];

/// 它能干什么。Cursor 那一列今天是空的,如实留空,不写「即将支持」。
/// 配置屏还没有「这个工具能干什么」这一列,所以暂时没有调用方。
#[allow(dead_code)]
pub const CAPABILITIES: &[&str] = &["注入技能", "恢复会话", "hooks 回传"];

/// 本机探到没探到。探不到就返回 `None`,不猜一个路径出来。
pub fn detect() -> Option<String> {
    v4_engine::resolve_claude_binary(None)
}
