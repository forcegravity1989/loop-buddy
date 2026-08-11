//! 字节泵(next 切片 5.5,design-s5-hexpanel.md §5.3/plan/23 §7 主控裁决
//! 7):v1 把「每 100ms 抽一次 PTY 字节」这条节奏挂在壳的主循环里(`app-
//! desktop/src/kernel.rs` 的 `pty_ticker`)——这正是 plan/23 §9 点名的结构
//! 病(界面层调度时钟),`scripts/guard-shell-no-clock.sh` 禁的就是这个。
//! 这份文件把同一条节奏原样搬过来,只是换了个住址:泵在这里(agentcli
//! 层,`bw-engine`),壳(`app-desktop`)经
//! `bw_connector::caps::Interactive::subscribe` 只订阅,不拉取,也不建
//! 时钟。
//!
//! [`crate::terminal_manager::TerminalManager`] 本身不改一行(硬约束——它
//! 是切片一B 已经移植好的移植件)。`drain_events` 是一个纯拉取方法,天生
//! 需要有人在外面按节奏调它;这份文件就是那个「外面」。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bw_core::ConversationId;
use tokio::sync::broadcast;

use crate::terminal_manager::TerminalManager;

/// v1 同款节奏(`kernel.rs` 原文注释:「100ms = 10fps,足够渲染终端输
/// 出;人眼读字的速度大约每秒 10 个字符,每 100ms 一批 8KB 绰绰有余」)。
/// 原样沿用,不重新调参——这是搬运,不是重新发明。
pub const PUMP_INTERVAL: Duration = Duration::from_millis(100);

/// 起一条泵任务:每 [`PUMP_INTERVAL`] 抽一次 `TerminalManager::drain_events`,
/// 按会话 id 分发进各自的广播频道——`byte_senders` 由
/// [`super::connector::AgentCliConnector`] 在 `start` 里为每个新会话开一
/// 条、在会话关闭时摘掉(`Interactive::subscribe` 就是从这张表按会话 id
/// 拿一份 `Receiver`)。
///
/// **这份文件是壳「零时钟」这条硬约束在引擎侧的落点**:定时器构造只准出
/// 现在这里,不许出现在 `app-desktop/src` 里——`scripts/guard-shell-no-
/// clock.sh` 只查后者,但语义上两边一致:抽取节奏归引擎侧持有,壳永远只
/// 收流,不建时钟。
///
/// **不做显式停止**(如实标注,不是漏做):调用方(`AgentCliConnector::
/// new`)起了这个任务就不再持有停止它的把手,任务活到进程退出为止。这
/// 与 `bw-app::run::RunManager` 用 `CancellationToken` + `Drop` 显式收尾
/// 的既有先例不同,取舍是:`AgentCliConnector` 在生产路径上注册进连接器
/// 注册表后本来就活到进程退出(没有「中途整个连接器不再需要」这个场
/// 景),指挥器/测试进程用完即整体退出——泄漏的是一个到时会被操作系统
/// 回收的 tokio 任务,不是需要显式收尾才对得起的资源。真出现「同一进程
/// 里反复创建又丢弃 `AgentCliConnector`」的场景,这条取舍要重新评估。
pub fn spawn_pump(
    terminals: Arc<Mutex<TerminalManager>>,
    byte_senders: Arc<Mutex<HashMap<ConversationId, broadcast::Sender<Vec<u8>>>>>,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(PUMP_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let batches = {
                let mut t = terminals.lock().expect("terminal manager mutex poisoned");
                t.drain_events()
            };
            if batches.is_empty() {
                continue;
            }
            let senders = byte_senders.lock().expect("byte senders mutex poisoned");
            for (id, bytes) in batches {
                if let Some(tx) = senders.get(&id) {
                    // 没有订阅方时 `send` 报 `Err`——同 v1「静默 tick 不发
                    // 送就是白发」的口径,如实丢弃。这批字节已经被
                    // `drain_events` 从有界输出环里吐出来了,「有没有人在
                    // 听」不是这个泵任务的责任范围(design §12 第 4 条如
                    // 实记的正是这条:没有订阅方时谁也不丢字节的保证)。
                    let _ = tx.send(bytes);
                }
            }
        }
    });
}
