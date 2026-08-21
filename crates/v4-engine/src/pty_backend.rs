//! PTY 平台接缝:内嵌终端怎么起子进程、双向倒腾字节、改尺寸、收尾杀进程。
//!
//! 之前 `InteractiveCliExecutor::run_skill_pty` 整个函数体挂 `#[cfg(windows)]`
//! (conpty-oxide),非 Windows 没有实现,直接落到 trait 默认的「PTY not
//! supported」——桌面壳又无条件走 PTY 路径,结果 macOS 上 ▶跑 第一步就报错。
//! 本模块把平台分叉从函数体里剥出来,变成同一个 [`PtyBackend`] trait 的两份
//! 平台实现:
//!
//! - [`windows::WindowsPtyBackend`] —— conpty-oxide,原函数体搬过来。相对搬迁
//!   前的原函数体,改动有四处,逐条列出好对账:① 「读 `self.claude_binary`
//!   字段」改成「读调用方传进来的 `binary` 参数」;② 子进程环境按下方那条
//!   规矩处理(2026-08-18 合入 main V3 的写法后与 main 一致:`.cmd`/`.bat` 用
//!   `cmd.exe /c` 托起、起步前先查工作目录与执行器路径存在、spawn 错误带
//!   OS 错误码与 cwd);③ 键盘字节改由独立写线程落盘(见下「写线程」);④ 收尾按
//!   `JoinHandle::is_finished()` 判断读线程是否已被 `select!` 轮询到 Ready,
//!   不再二次 `.await`(tokio 会 panic「JoinHandle polled after completion」)。
//! - [`unix::UnixPtyBackend`] —— portable-pty,补上 macOS/Linux 那一半。
//!
//! 两份后端的子进程环境都是「继承当前进程环境,再摘掉嵌套会话变量」
//! (`interactive_cli::apply_child_env`,与系统终端那条 tokio `Command` 路径同一
//! 张清单):不整份 `env_clear()` 再回放 `LaunchPlan.env`——V3 在 Windows 真机
//! 踩到 windows-spawn 拒绝 `=C:` 这类隐藏环境名(2026-08 main `3410401`),
//! 回放整张表反而起不来;只 `env_remove` 被禁的那些名字(`CLAUDE…` 打头的一族
//! 加上厂商那三个),其余原样继承。`plan.env` 仍是那份快照,供测试与读回核对,
//! 不再由后端回放。
//!
//! **写线程**:往 PTY 主端写键盘字节是同步阻塞的 fd 写(portable-pty 与
//! conpty-oxide 都不开非阻塞),而这个 `run` future 被 bw-app `tokio::spawn`
//! 到桌面壳内核的 **current_thread** 运行时上——子进程卡住不读输入、用户又
//! 粘贴超过 tty 输入队列容量(macOS 上几 KB)时,一次阻塞写会把整个内核线程
//! (所有 Command/Event)一起卡死。所以两份后端都把写端交给一条
//! `spawn_blocking` 线程,`select!` 循环里只往 std mpsc 通道塞字节,顺序不变。
//!
//! 调用方只认 [`PtyBackend`];[`active`] 按编译目标选一份实现,
//! `interactive_cli.rs` 里不再出现任何平台 `cfg`。
//!
//! 与 `SkillOutput` 的语义边界:这里的 `completed` 恒为 `true`,只表达「PTY 子
//! 进程退出了」这一件事实(读到 EOF 或读错误都算),不表示「活干成了」——那由
//! 更上层(评审 → 人点完成)判定。已知粗粒度:退出码不看、读错误与正常退出
//! 不分,上层把它当 `run_ok` 记进队友战绩;细化留在 docs/LEFTOVERS.md 减负-17。
//!
//! 两份后端的 `select!` 主循环长得几乎一样,刻意不抽公共骨架:两边的
//! 子进程/读端/写端/尺寸类型完全不同,硬抽会变成一堆泛型参数,可读性反而差;
//! 共用的只有 [`SUBMIT_READY_DELAY`] 这个常量。

use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::interactive_cli::{LaunchPlan, PtyInput, SkillOutput};
use crate::ExecError;

/// 首启后等多久再替用户按一次 Enter(`\r`)提交位置 prompt。
///
/// claude 交互式启动时位置 argv 不会自动提交(buddy 的 GLM 网关环境下实测),
/// TUI 起来后停在输入框等 Enter。这是启发式(赌 TUI 到这个点已加载完),不是
/// 侦测到了就绪信号;更慢的网络/机器上可能不够,失效模式是 `\r` 送进一个还
/// 没起来的输入框、等于没发。两份后端共用同一个值(docs/LEFTOVERS.md 减负-15)。
pub const SUBMIT_READY_DELAY: Duration = Duration::from_millis(2000);

/// PTY 平台接缝。只管四件事:起进程、双向倒腾字节、改尺寸、收尾杀进程。
#[async_trait]
pub trait PtyBackend: Send + Sync {
    /// 起一个 PTY 子进程跑 `binary` + `plan.args`(`env`/`cwd` 同样取自
    /// `plan`),把子进程输出经 `bytes_tx` 转发给调用方,从 `input_rx` 接收
    /// 键盘字节与 resize 请求。子进程退出(读到 EOF/读错误)后收尾返回。
    ///
    /// 这个 future 被中途丢弃(调用方 `JoinHandle::abort()`)时,子进程也必须
    /// 被收尾——Windows 靠 conpty-oxide 托管会话的 kill-on-drop Job,Unix 靠
    /// `unix::ChildGuard` 的 `Drop`。
    async fn run(
        &self,
        binary: &str,
        plan: &LaunchPlan,
        bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
        input_rx: mpsc::UnboundedReceiver<PtyInput>,
    ) -> Result<SkillOutput, ExecError>;
}

// 非 unix/非 windows 目标(例如 wasm32)在编译期挑明「PTY 后端只做了两家」,
// 不留一个含糊的兜底分支。bw-engine 本就不在 wasm32 keepalive 集合里。
#[cfg(not(any(unix, windows)))]
compile_error!("PTY 后端仅支持 unix/windows");

/// 按当前编译目标选可用的 PTY 后端。两个 `cfg` 互斥;两个后端都是零字段
/// 单元结构体,用 `impl Trait` 返回即可,不需要 `Box<dyn>`。
pub fn active() -> impl PtyBackend {
    #[cfg(windows)]
    {
        windows::WindowsPtyBackend
    }
    #[cfg(unix)]
    {
        unix::UnixPtyBackend
    }
}

#[cfg(windows)]
pub mod windows {
    //! Windows PTY 后端:conpty-oxide(portable-pty 0.9.0 的 ConPTY 实现不
    //! 把子进程 stdout 送到读端,见 docs/v1-prototype/issue2-metrics-interactive-loop.md §9)。
    //!
    //! **如实标注**:本仓开发机是 macOS,**这份文件自己没在 Windows 真机上跑
    //! 过**,只经 `cargo check --target x86_64-pc-windows-gnu -p v4-engine` 交叉
    //! 编译检查(2026-08-21 复核通过;此前这里写的是 `-p bw-engine`,那是另一
    //! 个 crate 的证据,标错了)。
    //!
    //! 能借的证据:这份与 V3 的 `bw-engine/src/pty_backend.rs` **逐字相同**
    //! (`diff` 无输出,2026-08-21 核过),所以那份跑出来的里程对这份成立。
    //!
    //! 中途丢弃(`abort`)时的收尾靠 conpty-oxide 自己:`Command::spawn()`
    //! 返回的是托管会话,其 `Child` 一律 kill-on-drop 且 Job 带
    //! kill-on-close,future 被丢弃 → `child` 被丢弃 → 整棵进程树被内核终止。

    use std::io::{Read, Write};
    use std::time::Duration;

    use async_trait::async_trait;
    use conpty_oxide::{blocking::Command as PtyCommand, Size};
    use tokio::sync::mpsc;

    use super::{PtyBackend, SUBMIT_READY_DELAY};
    use crate::interactive_cli::{apply_child_env, EnvSink, LaunchPlan, PtyInput, SkillOutput};
    use crate::ExecError;

    /// Same strip list as the tokio `Command` paths — see
    /// `interactive_cli::apply_child_env` (never replay the full process map
    /// through conpty-oxide: windows-spawn rejects hidden `=C:` names).
    impl EnvSink for PtyCommand {
        fn remove_env(&mut self, key: &str) {
            self.env_remove(key);
        }
    }

    pub struct WindowsPtyBackend;

    #[async_trait]
    impl PtyBackend for WindowsPtyBackend {
        /// Flow (conpty-oxide `blocking::Command` + `Session::into_parts`):
        ///  - `Command::new(binary).args().env().current_dir().spawn()` → `Session`
        ///  - `Session::into_parts()` → `{ child, output, input, controller }`
        ///  - `output` (OwnedReadHalf: std::io::Read) → spawn_blocking read loop →
        ///    `bytes_tx` (drains ConPTY output so the pipe can't fill —
        ///    `Child::wait` deadlocks otherwise, per conpty-oxide module docs)
        ///  - `input` (OwnedWriteHalf: std::io::Write) ← spawn_blocking write
        ///    loop ← `PtyInput::Bytes`(写线程,见模块文档)
        ///  - `controller.resize(Size)` ← `PtyInput::Resize`
        ///  - on first run (`plan.submit_prompt`), send `\r` after a brief
        ///    ready-wait to submit the positional skill body (claude interactive
        ///    positional argv doesn't auto-submit in buddy's GLM-gateway env).
        ///  - Child exit (EOF on output) → kill (idempotent) → return completed.
        async fn run(
            &self,
            binary: &str,
            plan: &LaunchPlan,
            bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
            mut input_rx: mpsc::UnboundedReceiver<PtyInput>,
        ) -> Result<SkillOutput, ExecError> {
            // Build the spawn command. conpty-oxide's Command mirrors
            // std::process::Command (builder takes &mut self). The child's stdio
            // is the pseudoconsole (handles set to INVALID_HANDLE_VALUE — a
            // redirected parent cannot leak its stdio into the child), and no
            // handles are inherited (a leaked output pipe copy would keep the
            // session from ever reaching EOF).
            if !plan.cwd.as_os_str().is_empty() && !plan.cwd.is_dir() {
                return Err(ExecError::Failed(format!(
                    "conpty-oxide spawn failed: 工作目录不存在 {}（无法启动 {binary}）",
                    plan.cwd.display()
                )));
            }
            if !std::path::Path::new(binary).is_file() && std::path::Path::new(binary).is_absolute()
            {
                return Err(ExecError::Failed(format!(
                    "conpty-oxide spawn failed: 找不到执行器 {binary}"
                )));
            }

            // `.cmd`/`.bat` are not PE images — CreateProcess fails unless we
            // host them with cmd.exe. win_cmd::tokio_cmd does the same wrap;
            // ConPTY must not go through that helper (CREATE_NO_WINDOW).
            let mut cmd = if crate::win_cmd::is_windows_script(binary) {
                let mut c = PtyCommand::new("cmd.exe");
                c.arg("/c");
                c.arg(binary);
                c
            } else {
                PtyCommand::new(binary)
            };
            cmd.args(&plan.args);
            apply_child_env(&mut cmd);
            cmd.current_dir(&plan.cwd);

            // Spawn the managed ConPTY session. The returned Session owns its
            // pseudoconsole, I/O, child process, and a kill-on-close Job. into_parts
            // separates ownership without detaching the child.
            let session = cmd.spawn().map_err(|e| {
                let os = e.io_error().map(|i| format!("; {i}")).unwrap_or_default();
                ExecError::Failed(format!(
                    "conpty-oxide spawn failed: {e}{os}; cwd={}",
                    plan.cwd.display()
                ))
            })?;
            let parts = session.into_parts();
            let mut child = parts.child;
            let output = parts.output; // moved into the read thread below
            let writer = parts.input; // moved into the write thread below
            let controller = parts.controller; // Clone; resize/clear control

            // Read loop (blocking, spawn_blocking): drains ConPTY output → bytes_tx.
            // MUST run on a separate thread: conpty-oxide's module docs warn that
            // Child::wait (and the console host generally) can stop making progress
            // once conout's pipe buffer fills, so a caller that blocks without
            // draining output can deadlock.
            let read_handle = tokio::task::spawn_blocking(move || {
                let mut reader = output;
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF — child exited (broken pipe → Ok(0))
                        Ok(n) => {
                            if bytes_tx.send(buf[..n].to_vec()).is_err() {
                                break; // App dropped the receiver — stop reading
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // Write loop (blocking, spawn_blocking): `select!` 里只往通道塞字节,
            // 真正的同步 write 在这条线程上做(见模块文档「写线程」)。通道发送端
            // 一丢(下面收尾处),线程写完余下字节就退出,`writer` 随之释放
            // ——conin 关闭、伪控制台收尾,和搬迁前「writer 持到函数末尾」等价。
            let (write_tx, write_rx) = std::sync::mpsc::channel::<Vec<u8>>();
            let write_handle = tokio::task::spawn_blocking(move || {
                let mut writer = writer;
                for bytes in write_rx {
                    // OwnedWriteHalf: bytes written become console input;
                    // line-oriented programs expect `\r\n`. flush is a no-op.
                    let _ = writer.write_all(&bytes);
                    let _ = writer.flush();
                }
            });

            // claude interactive positional argv doesn't auto-submit in
            // buddy's GLM-gateway environment — the TUI starts and waits for
            // Enter. On first run (submit_prompt), wait briefly for the TUI to
            // load, then send `\r` to submit the positional skill body. The read
            // loop drains concurrently so the pipe can't fill during this wait.
            // Resume (submit_prompt=false) has no positional — nothing to submit.
            let submit_delay = tokio::time::sleep(SUBMIT_READY_DELAY);
            tokio::pin!(submit_delay);
            let mut submitted = !plan.submit_prompt;

            tokio::pin!(read_handle);
            loop {
                tokio::select! {
                    // Read loop finished (child exited / EOF / App dropped bytes_rx).
                    _ = &mut read_handle => break,
                    // User input from the App (typed bytes or resize).
                    input = input_rx.recv() => match input {
                        Some(PtyInput::Bytes(bytes)) => {
                            let _ = write_tx.send(bytes);
                        }
                        Some(PtyInput::Resize { cols, rows }) => {
                            // Size::try_new takes (cols, rows) — cols first.
                            if let Ok(size) = Size::try_new(cols, rows) {
                                let _ = controller.resize(size);
                            }
                        }
                        None => break, // App dropped the input sender — stop.
                    },
                    // Submit the positional prompt after the TUI loads (first run).
                    _ = &mut submit_delay, if !submitted => {
                        submitted = true;
                        let _ = write_tx.send(b"\r".to_vec());
                    }
                }
            }

            // Teardown: kill the child tree (idempotent — no-op if it already
            // exited via EOF). This unblocks the read thread if it's still
            // draining (kill → child exit → output EOF → read thread returns).
            // 读线程若已在 `select!` 里被轮询到 Ready,不能再 `.await` 一次
            // (tokio panic「JoinHandle polled after completion」)——按
            // `is_finished()` 判断,与 Unix 后端同一处理。
            let _ = child.kill();
            if !read_handle.is_finished() {
                let _ = tokio::time::timeout(Duration::from_secs(2), read_handle).await;
            }
            // 丢掉发送端 → 写线程写完余下字节退出 → `writer` 释放(conin 关闭)。
            // `controller` 随后在函数末尾释放,伪控制台收尾。
            drop(write_tx);
            let _ = tokio::time::timeout(Duration::from_secs(2), write_handle).await;

            Ok(SkillOutput {
                completed: true,
                summary: "(pty session ended)".to_string(),
            })
        }
    }
}

#[cfg(unix)]
pub mod unix {
    //! Unix(macOS/Linux)PTY 后端:portable-pty。范围与 Windows 后端对齐:
    //! spawn / 字节双向 / 尺寸 / 首启自动提交位置 prompt / 收尾杀进程。
    //!
    //! 与 Windows 后端的两处刻意差异:
    //!
    //! 1. 收尾按**进程组**杀(`SIGHUP` 宽限最多 200ms 后 `SIGKILL`),不是只对
    //!    顶层 pid 发信号——portable-pty 在 `pre_exec` 里对子进程调了
    //!    `setsid()`,子进程 pid 即进程组号,只杀顶层 pid 连坐不到 `nohup … &`
    //!    这类脱离父进程独立存活的孙进程。Windows 侧 conpty-oxide 的 Job
    //!    Object 是否已等价覆盖,没有 Windows 机器验证不了,如实留白。
    //! 2. 中途丢弃要自己兜:portable-pty 的 Unix 子进程就是裸的
    //!    `std::process::Child`,不 kill-on-drop;读线程又持有一份 dup 出来的主
    //!    端 fd,future 被丢弃后主端并不会真正关闭,子进程收不到挂断信号。
    //!    bw-app 的 `cancel_run` 正是用 `JoinHandle::abort()` 丢弃这个 future,
    //!    所以子进程句柄套在 [`ChildGuard`] 里,`Drop` 时若尚未正常收尾,就丢给
    //!    一条独立线程按进程组杀掉并 `wait()` 收割(不能在 `Drop` 里阻塞等宽限:
    //!    丢弃方可能是内核唯一那条线程)。早退错误路径也靠它,不再手写 `kill()`。

    use std::io::{Read, Write};
    use std::time::Duration;

    use async_trait::async_trait;
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;
    use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
    use tokio::sync::mpsc;

    use super::{PtyBackend, SUBMIT_READY_DELAY};
    use crate::interactive_cli::{apply_child_env, EnvSink, LaunchPlan, PtyInput, SkillOutput};
    use crate::ExecError;

    /// Same strip list as the tokio `Command` paths and the Windows backend —
    /// see `interactive_cli::apply_child_env`.
    impl EnvSink for CommandBuilder {
        fn remove_env(&mut self, key: &str) {
            self.env_remove(key);
        }
    }

    pub struct UnixPtyBackend;

    /// 子进程句柄的持有者:正常收尾时用 [`ChildGuard::take`] 把句柄拿走、
    /// 在 `spawn_blocking` 里从容收尾;没拿走就被丢弃(`abort` / 早退错误路径)
    /// 时,`Drop` 起独立线程完成同一套收尾——子进程绝不因为 future 被丢弃而
    /// 变成孤儿(见模块文档第 2 条)。
    struct ChildGuard(Option<Box<dyn Child + Send + Sync>>);

    impl ChildGuard {
        fn take(&mut self) -> Option<Box<dyn Child + Send + Sync>> {
            self.0.take()
        }
    }

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                std::thread::spawn(move || teardown_group(child.as_mut()));
            }
        }
    }

    #[async_trait]
    impl PtyBackend for UnixPtyBackend {
        async fn run(
            &self,
            binary: &str,
            plan: &LaunchPlan,
            bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
            mut input_rx: mpsc::UnboundedReceiver<PtyInput>,
        ) -> Result<SkillOutput, ExecError> {
            // 初始尺寸给常见默认值——真实尺寸由调用方经 `PtyInput::Resize`
            // 补(桌面壳 attach 时会立刻发一次)。
            let pty_system = native_pty_system();
            let pair = pty_system
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| ExecError::Failed(format!("portable-pty openpty failed: {e}")))?;

            let mut cmd = CommandBuilder::new(binary);
            for a in &plan.args {
                cmd.arg(a);
            }
            // `CommandBuilder::new` 起步就整份拷贝了当前进程环境;只摘掉嵌套
            // 会话变量(与 tokio Command / Windows 后端同一张清单,见模块文档)。
            apply_child_env(&mut cmd);
            cmd.cwd(&plan.cwd);

            let child = pair
                .slave
                .spawn_command(cmd)
                .map_err(|e| ExecError::Failed(format!("portable-pty spawn failed: {e}")))?;
            // 从这一行起,任何提前 `return`/`?`/future 被丢弃,都由 guard 收尾。
            let mut guard = ChildGuard(Some(child));
            // 子进程自己持有一份从端;父进程这份必须释放,否则哪怕子进程已
            // 退出,只要还有一个从端引用活着,主端读侧永远等不到 EOF。
            drop(pair.slave);

            let mut reader = pair
                .master
                .try_clone_reader()
                .map_err(|e| ExecError::Failed(format!("portable-pty clone reader failed: {e}")))?;
            let writer = pair
                .master
                .take_writer()
                .map_err(|e| ExecError::Failed(format!("portable-pty take writer failed: {e}")))?;

            // 读循环(阻塞,spawn_blocking):drain 主端输出 → bytes_tx;读到
            // 0 字节或读错误(Unix 上从端全关后主端常读到 EIO)都当「子进程
            // 退出」处理。
            let read_handle = tokio::task::spawn_blocking(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if bytes_tx.send(buf[..n].to_vec()).is_err() {
                                break; // 调用方丢了接收端 —— 停止读
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // 写循环(阻塞,spawn_blocking):见模块文档「写线程」。
            let (write_tx, write_rx) = std::sync::mpsc::channel::<Vec<u8>>();
            let write_handle = tokio::task::spawn_blocking(move || {
                let mut writer = writer;
                for bytes in write_rx {
                    let _ = writer.write_all(&bytes);
                    let _ = writer.flush();
                }
            });

            // 首启:位置 prompt 已以 argv 传给子进程,但没人替它按 Enter——等
            // TUI 加载完发一次 `\r`;续接(`submit_prompt == false`)不发。
            let submit_delay = tokio::time::sleep(SUBMIT_READY_DELAY);
            tokio::pin!(submit_delay);
            let mut submitted = !plan.submit_prompt;

            tokio::pin!(read_handle);
            loop {
                tokio::select! {
                    _ = &mut read_handle => break,
                    input = input_rx.recv() => match input {
                        Some(PtyInput::Bytes(bytes)) => {
                            let _ = write_tx.send(bytes);
                        }
                        Some(PtyInput::Resize { cols, rows }) => {
                            let _ = pair.master.resize(PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                        None => break, // 调用方丢了输入端 —— 停止。
                    },
                    _ = &mut submit_delay, if !submitted => {
                        submitted = true;
                        let _ = write_tx.send(b"\r".to_vec());
                    },
                }
            }

            // 收尾:按进程组杀 + `wait()` 收割退出状态(只 kill 不 wait 会留
            // 僵尸表项)。两者都是同步阻塞调用,挪进 spawn_blocking,不占
            // 内核线程。句柄从 guard 里拿走,guard 的 `Drop` 于是成为 no-op。
            if let Some(mut child) = guard.take() {
                let _ = tokio::task::spawn_blocking(move || teardown_group(child.as_mut())).await;
            }

            // 读线程若已在 `select!` 里被轮询到 Ready,不能再 `.await` 一次
            // (tokio panic「JoinHandle polled after completion」;`bash -c
            // 'echo pty-ok'` 这种立刻退出的子进程百分之百触发)。这个 2s
            // timeout 只是不再等 `JoinHandle`,并不能真的打断还卡在同步
            // `reader.read()` 上的 OS 线程——上面已 killpg 过,主端理应很快
            // 等到 EOF/EIO 让读线程自己退出。
            if !read_handle.is_finished() {
                let _ = tokio::time::timeout(Duration::from_secs(2), read_handle).await;
            }
            // 丢掉发送端 → 写线程写完余下字节退出 → `writer` 释放。
            drop(write_tx);
            let _ = tokio::time::timeout(Duration::from_secs(2), write_handle).await;

            Ok(SkillOutput {
                completed: true,
                summary: "(pty session ended)".to_string(),
            })
        }
    }

    /// 按进程组「先礼后兵」收尾并收割:首领还活着就先 `SIGHUP`(给子孙进程一
    /// 个自行清理退出的机会),宽限**最多** 200ms、首领一退就不再干等;然后无
    /// 论首领是否已退都补一刀 `SIGKILL` 连坐还活着的孙进程;最后 `wait()` 收
    /// 割首领的退出状态(已被 `try_wait` 收割过的话拿到的是缓存值)。
    ///
    /// 首领已经经 EOF 路径自然退出(最常见的收尾)时,`try_wait` 第一下就命中,
    /// 整个函数不睡一毫秒。
    ///
    /// `pid` 就是目标进程组号——portable-pty 的 unix 实现在
    /// `CommandBuilder::spawn_command` 的 `pre_exec` 里对子进程调了
    /// `setsid()`,子进程同时成为新会话与新进程组首领,`pid == pgid`。
    ///
    /// `killpg` 对已不存在的进程组返回 `ESRCH`——进程组已空的正常情况,静默
    /// 吞掉。用 `nix` 的安全封装而不是 `libc` 裸调用:bw-engine 整 crate
    /// `#![forbid(unsafe_code)]`,不为一个 syscall 开口子;`nix` 本就在锁文件
    /// 里(portable-pty 自身依赖)。
    fn teardown_group(child: &mut (dyn Child + Send + Sync)) {
        let pgid = child.process_id().map(|pid| Pid::from_raw(pid as i32));
        let leader_exited = matches!(child.try_wait(), Ok(Some(_)));
        if let Some(pgid) = pgid {
            if !leader_exited {
                let _ = killpg(pgid, Signal::SIGHUP);
                for _ in 0..10 {
                    std::thread::sleep(Duration::from_millis(20));
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        break;
                    }
                }
            }
            let _ = killpg(pgid, Signal::SIGKILL);
        }
        let _ = child.wait();
    }
}
