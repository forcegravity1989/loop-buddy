//! V1 终端会话重构 · 集中模块 TerminalManager(深模块 seam,设计 md §7.1)。
//!
//! 外部调用者只认识稳定的 [`ConversationId`],不直接碰 child handle、channel、
//! 平台 PTY 类型。字节按会话身份路由;每会话有界输出缓冲(满丢最老)。
//!
//! **阶段·底座**:接 PTY channels + 有界缓冲 + resize/input/drain。多会话
//! Map 已就位;同项目「只一件交付」仍由 `active_run` 串行——并发常驻是下一
//! 阶段。进程退出即清空(纯内存),身份从 `claude_conversation` 表恢复。
//! 详见 `docs/v1-prototype/issue2-terminal-conversation-refactor.md` §7 / §10.1。

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bw_core::ConversationId;
use tokio::sync::mpsc;

use crate::interactive_cli::PtyInput;

/// 每会话输出批次数上限(设计 md §7.4)。满了丢最老,避免背压卡住 claude。
pub const OUTPUT_BATCH_CAP: usize = 64;

/// 单批字节上限(设计 md §7.4:每批 ≤8KB)。读侧超长则切段入队。
pub const OUTPUT_BATCH_MAX_BYTES: usize = 8 * 1024;

/// 某个会话此刻在内存里持有的身份事实(可随 attach 写入)。
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

/// 有界输出环:满了 pop 最老。
#[derive(Debug, Default)]
struct SessionOutputBuffer {
    batches: VecDeque<Vec<u8>>,
}

impl SessionOutputBuffer {
    fn push_chunk(&mut self, mut chunk: Vec<u8>) {
        // 超长批切段,每段单独占一槽(仍受 64 槽上限约束)。
        while !chunk.is_empty() {
            let take = chunk.len().min(OUTPUT_BATCH_MAX_BYTES);
            let piece = chunk.drain(..take).collect::<Vec<_>>();
            if self.batches.len() >= OUTPUT_BATCH_CAP {
                self.batches.pop_front();
            }
            self.batches.push_back(piece);
        }
    }

    fn drain_concat(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(b) = self.batches.pop_front() {
            out.extend_from_slice(&b);
        }
        out
    }
}

/// 一个活着的终端连接(纯内存;PTY 子进程在 `run_skill_pty` 任务里)。
pub struct TerminalSession {
    pub conversation_id: ConversationId,
    pty_input_tx: mpsc::UnboundedSender<PtyInput>,
    output: Arc<Mutex<SessionOutputBuffer>>,
    pub current_size: (u16, u16),
    pub meta: ConversationMeta,
}

/// 集中模块:多会话终端管理器。
///
/// 不碰 dioxus/tauri/wry(过 `guard-kernel-ui-free.sh`)。Windows PTY 本体仍
/// 在 `InteractiveCliExecutor::run_skill_pty`;本模块管身份、channel、缓冲、
/// 尺寸。Unix adapter 后续按 PtyBackend seam 补(本阶段不实现)。
#[derive(Default)]
pub struct TerminalManager {
    sessions: HashMap<ConversationId, TerminalSession>,
    /// UI 最近一次 fit 的尺寸;下次 attach 作初始 resize(修窄窗错行的一部分)。
    last_fit_size: Option<(u16, u16)>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录 UI fit 到的真实尺寸(无会话时也能记,供下次 spawn)。
    pub fn note_fit_size(&mut self, cols: u16, rows: u16) {
        if cols > 0 && rows > 0 {
            self.last_fit_size = Some((cols, rows));
        }
    }

    pub fn last_fit_size(&self) -> Option<(u16, u16)> {
        self.last_fit_size
    }

    /// 是否有任一活着的 PTY 连接(驱动 UI `pty_active`)。
    pub fn has_live(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// 当前活连接的 conversation id 列表(底座阶段通常 0/1 个)。
    pub fn live_ids(&self) -> Vec<ConversationId> {
        self.sessions.keys().copied().collect()
    }

    /// 若恰好一个活连接,返回其 id(底座单 PTY 时给 UI 用)。
    pub fn sole_live_id(&self) -> Option<ConversationId> {
        if self.sessions.len() == 1 {
            self.sessions.keys().next().copied()
        } else {
            None
        }
    }

    /// 注册一次 PTY 会话并返回交给 `run_skill_pty` 的两端。
    ///
    /// **底座约束**:同刻只保留一个活 PTY——attach 新会话前关掉其余(并发
    /// 常驻是下一阶段)。有界缓冲在 Manager 侧;PTY 读循环仍用 unbounded
    /// 推入,forwarder 写入环并丢最老。
    ///
    /// 返回 `(bytes_tx, input_rx)`:bytes_tx 给 PTY 读线程;input_rx 给
    /// executor 的 select 循环。
    pub fn attach(
        &mut self,
        conversation_id: ConversationId,
        meta: ConversationMeta,
        initial_size: Option<(u16, u16)>,
    ) -> (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<PtyInput>,
    ) {
        // 底座:新交付替换旧连接(不留无身份全局槽的兼容层)。
        let stale: Vec<ConversationId> = self
            .sessions
            .keys()
            .copied()
            .filter(|id| *id != conversation_id)
            .collect();
        for id in stale {
            self.close(id);
        }
        self.close(conversation_id);

        let size = initial_size.or(self.last_fit_size).unwrap_or((80, 24));

        let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();
        let output = Arc::new(Mutex::new(SessionOutputBuffer::default()));
        let output_fwd = Arc::clone(&output);

        // Forwarder: PTY 无界推入 → 有界环(满丢最老)。必须在 runtime 内。
        tokio::spawn(async move {
            while let Some(batch) = bytes_rx.recv().await {
                if let Ok(mut buf) = output_fwd.lock() {
                    buf.push_chunk(batch);
                }
            }
        });

        // 初始尺寸立刻进 input 队列,executor select 起来就会 resize
        // (不再默默停在 ConPTY 默认 80×24)。
        if size.0 > 0 && size.1 > 0 {
            let _ = input_tx.send(PtyInput::Resize {
                cols: size.0,
                rows: size.1,
            });
        }

        self.sessions.insert(
            conversation_id,
            TerminalSession {
                conversation_id,
                pty_input_tx: input_tx,
                output,
                current_size: size,
                meta,
            },
        );

        (bytes_tx, input_rx)
    }

    /// 向指定会话写键盘字节或 resize。会话不存在 → false。
    pub fn input(&self, conversation_id: ConversationId, input: PtyInput) -> bool {
        let Some(session) = self.sessions.get(&conversation_id) else {
            return false;
        };
        session.pty_input_tx.send(input).is_ok()
    }

    /// 更新会话尺寸并通知 PTY。同时记入 last_fit_size。
    pub fn resize(&mut self, conversation_id: ConversationId, cols: u16, rows: u16) -> bool {
        self.note_fit_size(cols, rows);
        let Some(session) = self.sessions.get_mut(&conversation_id) else {
            return false;
        };
        session.current_size = (cols, rows);
        session
            .pty_input_tx
            .send(PtyInput::Resize { cols, rows })
            .is_ok()
    }

    /// 关掉会话连接(丢 sender → executor 侧 input_rx 结束 → PTY 收尾)。
    pub fn close(&mut self, conversation_id: ConversationId) {
        self.sessions.remove(&conversation_id);
    }

    pub fn close_all(&mut self) {
        self.sessions.clear();
    }

    /// 抽出所有会话自上次 drain 以来的输出,每项带 conversation_id。
    pub fn drain_events(&mut self) -> Vec<(ConversationId, Vec<u8>)> {
        let mut out = Vec::new();
        for (id, session) in self.sessions.iter_mut() {
            if let Ok(mut buf) = session.output.lock() {
                let bytes = buf.drain_concat();
                if !bytes.is_empty() {
                    out.push((*id, bytes));
                }
            }
        }
        out
    }
}
