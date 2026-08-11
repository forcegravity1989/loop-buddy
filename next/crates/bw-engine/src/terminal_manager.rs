//! V1 终端会话重构 · 集中模块 TerminalManager(深模块 seam,设计 md §7.1)。
//!
//! 外部调用者只认识稳定的 [`ConversationId`],不直接碰 child handle、channel、
//! 平台 PTY 类型。字节按会话身份路由;每会话有界输出缓冲(满丢最老)。
//!
//! 并发切卡:多会话常驻 PTY,attach 不杀 peer;交付锁仍由 `active_run`。
//! 见 `docs/v1-prototype/issue2-terminal-conversation-refactor.md` §7 / §10.1。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use bw_core::ConversationId;
use tokio::sync::mpsc;

use crate::interactive_cli::PtyInput;

/// 每会话输出批次数上限(设计 md §7.4)。满了丢最老,避免背压卡住 claude。
pub const OUTPUT_BATCH_CAP: usize = 64;

/// 单批字节上限(设计 md §7.4:每批 ≤8KB)。读侧超长则切段入队。
pub const OUTPUT_BATCH_MAX_BYTES: usize = 8 * 1024;

/// 某个会话此刻在内存里持有的身份事实(可随 attach 写入)。
///
/// next 减法专项(2026-08):`claude_session_id`/`workspace_path`/
/// `branch_name` 三个只写不读字段已删——`attach()` 调用点真写入过它们,但
/// 没有任何读侧消费(死代码审计坐实,grep 复核零消费者)。`issue_id` 同样
/// 只写不读(恒 `IssueId::nil()`,波三复核 grep 零读者)已删,现在只剩身
/// 份键 `conversation_id`。
#[derive(Clone, Debug)]
pub struct ConversationMeta {
    pub conversation_id: ConversationId,
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
/// 不碰 dioxus/tauri/wry(过 `guard-kernel-ui-free.sh`)。PTY 本体在
/// `InteractiveCliExecutor::run_skill_pty` 经 [`crate::pty_backend`] 接缝分
/// 派到平台实现(Windows: conpty-oxide;Unix: portable-pty,next 切片三B
/// 补齐);本模块管身份、channel、缓冲、尺寸,不关心平台细节。
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

    /// 注册 PTY 会话,返回给 `run_skill_pty` 的两端。不关其它会话;同 id 重 spawn 先清自己。
    pub fn attach(
        &mut self,
        conversation_id: ConversationId,
        meta: ConversationMeta,
        initial_size: Option<(u16, u16)>,
    ) -> (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<PtyInput>,
    ) {
        // 同会话重 spawn:只关自己,不杀 peer。
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

    /// 某会话此刻记的尺寸(读回用——同 `resize()`/`attach()` 写入的
    /// `current_size` 字段)。next 切片 5.5 修(评审 task-s55-review.md
    /// Important-1):`terminal_feed_smoke` 用它核对 `Interactive::
    /// send_input(Resize)` 真的把尺寸账目写回了这里,不是只把字节塞进
    /// PTY。会话不存在 → `None`,同本模块其余查询方法的既有口径。
    pub fn current_size(&self, conversation_id: ConversationId) -> Option<(u16, u16)> {
        self.sessions.get(&conversation_id).map(|s| s.current_size)
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
