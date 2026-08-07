//! V1 终端会话重构 · 集中模块 TerminalManager(深模块 seam,设计 md §7.1)。
//!
//! 不再让多会话逻辑散在 `bw-app/lib.rs`、`kernel.rs`、`op.rs` 三处。这个
//! 模块把"对一个 PTY 进程的临时连接"的全部行为(spawn / resume / 字节路由
//! / resize / 重连)藏在一个小接口后面,外部调用者只认识稳定的
//! [`ConversationId`],不直接碰 child handle、channel、平台 PTY 类型。
//!
//! **阶段1 骨架**:只存会话身份(`HashMap<ConversationId, ConversationMeta>`),
//! 无 PTY spawn / input / resize / events —— 那是阶段2。进程退出即清空(纯
//! 内存),从 `claude_conversation` 表恢复身份。详见
//! `docs/v1-prototype/issue2-terminal-conversation-refactor.md` §7.1 / §10。

use std::collections::HashMap;
use std::path::PathBuf;

use bw_core::ConversationId;

/// 某个会话此刻在内存里持有的身份事实(纯内存,不进库)。阶段2 会加上
/// PTY input channel、子进程句柄、当前 cols/rows、当前是否显示等(见设计
/// md §7.1 `TerminalSession`);阶段1 只搬身份,这些字段先备着。
#[allow(dead_code)] // 阶段2 PTY 接入后这些字段才被读
#[derive(Clone, Debug)]
pub struct ConversationMeta {
    pub conversation_id: ConversationId,
    /// claude CLI 的 `--resume` id(hook 回传)。空 = 首次未捕获。
    pub claude_session_id: String,
    /// 固定 worktree 路径(重启 resume 重建 worktree 用同路径)。
    pub workspace_path: PathBuf,
    /// 该活分支 `bw/issue-<github_number>`。
    pub branch_name: String,
}

/// 集中模块:多会话终端管理器。外部调用者只认识稳定的 `ConversationId`,
/// 不直接碰 child handle / channel / 平台 PTY 类型(设计 md §7.1 深模块)。
///
/// 阶段1 骨架:只存会话身份,无 PTY spawn / input / resize / events —— 那是
/// 阶段2。进程退出即清空(纯内存),从 `claude_conversation` 表恢复身份。
/// 不碰 dioxus/tauri/wry(过 `guard-kernel-ui-free.sh`)。
#[allow(dead_code)] // 阶段2 接入 PTY;阶段1 无调用点
#[derive(Clone, Debug, Default)]
pub struct TerminalManager {
    sessions: HashMap<ConversationId, ConversationMeta>,
}

impl TerminalManager {
    /// 空的管理器。
    pub fn new() -> Self {
        Self::default()
    }
}
