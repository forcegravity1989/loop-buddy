//! PTY 平台接缝:内嵌终端怎么起子进程、双向倒腾字节、改尺寸、收尾杀进程。
//!
//! 之前 `InteractiveCliExecutor::run_skill_pty` 整个函数体挂 `#[cfg(windows)]`
//! (conpty-oxide),非 Windows 没有实现,直接落到 trait 默认的「PTY not
//! supported」——桌面壳又无条件走 PTY 路径,结果 macOS 上 ▶跑 第一步就报错。
//! 本模块把平台分叉从函数体里剥出来,变成同一个 [`PtyBackend`] trait 的两份
//! 平台实现:
//!
//! - [`windows::WindowsPtyBackend`] —— conpty-oxide,原函数体整段搬过来,只把
//!   「读 `self.claude_binary` 字段」改成「读调用方传进来的 `binary` 参数」,
//!   外加一句 `env_clear()`(见下)。
//! - [`unix::UnixPtyBackend`] —— portable-pty,补上 macOS/Linux 那一半。
//!
//! 两份后端起步都 `env_clear()` 再逐条套用 `LaunchPlan.env`——`plan.env` 本身
//! 就是「当前进程全量环境 − 嵌套会话变量」的快照(见
//! `interactive_cli::build_startup_plan`),不清空的话子进程会先整份继承父进程
//! 环境,被删掉的 `ANTHROPIC_AUTH_TOKEN`/`CLAUDECODE` 等又原样漏回去。
//!
//! 调用方只认 [`PtyBackend`];[`active`] 按编译目标选一份实现,
//! `interactive_cli.rs` 里不再出现任何平台 `cfg`。
//!
//! 与 `SkillOutput` 的语义边界:这里的 `completed` 恒为 `true`,只表达「PTY 子
//! 进程退出了」这一件事实,不表示「活干成了」——那由更上层(评审 → 人点完成)
//! 判定。

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::interactive_cli::{LaunchPlan, PtyInput, SkillOutput};
use crate::ExecError;

/// PTY 平台接缝。只管四件事:起进程、双向倒腾字节、改尺寸、收尾杀进程。
#[async_trait]
pub trait PtyBackend: Send + Sync {
    /// 起一个 PTY 子进程跑 `binary` + `plan.args`(`env`/`cwd` 同样取自
    /// `plan`),把子进程输出经 `bytes_tx` 转发给调用方,从 `input_rx` 接收
    /// 键盘字节与 resize 请求。子进程退出(读到 EOF/读错误)后收尾返回。
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
    //! **如实标注**:本仓开发机是 macOS,这份实现只经
    //! `cargo check --target x86_64-pc-windows-gnu -p bw-engine` 交叉编译检查,
    //! 搬迁后未在 Windows 真机跑过。

    use std::io::{Read, Write};
    use std::time::Duration;

    use async_trait::async_trait;
    use conpty_oxide::{blocking::Command as PtyCommand, Size};
    use tokio::sync::mpsc;

    use super::PtyBackend;
    use crate::interactive_cli::{LaunchPlan, PtyInput, SkillOutput};
    use crate::ExecError;

    pub struct WindowsPtyBackend;

    #[async_trait]
    impl PtyBackend for WindowsPtyBackend {
        /// Flow (conpty-oxide `blocking::Command` + `Session::into_parts`):
        ///  - `Command::new(binary).args().env().current_dir().spawn()` → `Session`
        ///  - `Session::into_parts()` → `{ child, output, input, controller }`
        ///  - `output` (OwnedReadHalf: std::io::Read) → spawn_blocking read loop →
        ///    `bytes_tx` (drains ConPTY output so the pipe can't fill —
        ///    `Child::wait` deadlocks otherwise, per conpty-oxide module docs)
        ///  - `input` (OwnedWriteHalf: std::io::Write) ← `PtyInput::Bytes`
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
            let mut cmd = PtyCommand::new(binary);
            cmd.args(&plan.args);
            // `plan.env` 是子进程环境的唯一来源(见模块文档)。
            cmd.env_clear();
            for (k, v) in &plan.env {
                cmd.env(k, v);
            }
            cmd.current_dir(&plan.cwd);

            // Spawn the managed ConPTY session. The returned Session owns its
            // pseudoconsole, I/O, child process, and a kill-on-close Job. into_parts
            // separates ownership without detaching the child.
            let session = cmd
                .spawn()
                .map_err(|e| ExecError::Failed(format!("conpty-oxide spawn failed: {e}")))?;
            let parts = session.into_parts();
            let mut child = parts.child;
            let output = parts.output; // moved into the read thread below
            let mut writer = parts.input; // held until teardown (dropping it ends the session)
            let controller = parts.controller; // Clone; resize/clear control

            // Read loop (blocking, spawn_blocking): drains ConPTY output → bytes_tx.
            // MUST run on a separate thread: conpty-oxide's module docs warn that
            // Child::wait (and the console host generally) can stop making progress
            // once conout's pipe buffer fills, so a caller that blocks without
            // draining output can deadlock.
            let read_tx = bytes_tx.clone();
            let read_handle = tokio::task::spawn_blocking(move || {
                let mut reader = output;
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF — child exited (broken pipe → Ok(0))
                        Ok(n) => {
                            if read_tx.send(buf[..n].to_vec()).is_err() {
                                break; // App dropped the receiver — stop reading
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // claude interactive positional argv doesn't auto-submit in
            // buddy's GLM-gateway environment — the TUI starts and waits for
            // Enter. On first run (submit_prompt), wait briefly for the TUI to
            // load, then send `\r` to submit the positional skill body. The read
            // loop drains concurrently so the pipe can't fill during this wait.
            // Resume (submit_prompt=false) has no positional — nothing to submit.
            let submit_delay = tokio::time::sleep(Duration::from_millis(2000));
            tokio::pin!(submit_delay);
            let mut submitted = !plan.submit_prompt;

            // 读循环先结束时(子进程正常退出,最常见的收尾路径)`read_handle`
            // 已在 `select!` 里被轮询到 Ready,收尾不能再 `.await` 它一次
            // (tokio 会 panic「JoinHandle polled after completion」)——用
            // 标志位区分,与 Unix 后端同一处理。
            let mut read_finished = false;

            tokio::pin!(read_handle);
            loop {
                tokio::select! {
                    // Read loop finished (child exited / EOF / App dropped bytes_rx).
                    _ = &mut read_handle => {
                        read_finished = true;
                        break;
                    }
                    // User input from the App (typed bytes or resize).
                    input = input_rx.recv() => match input {
                        Some(PtyInput::Bytes(bytes)) => {
                            // OwnedWriteHalf: bytes written become console input;
                            // line-oriented programs expect `\r\n`. flush is a no-op.
                            let _ = writer.write_all(&bytes);
                            let _ = writer.flush();
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
                        let _ = writer.write_all(b"\r");
                        let _ = writer.flush();
                    }
                }
            }

            // Teardown: kill the child tree (idempotent — no-op if it already
            // exited via EOF). This unblocks the read thread if it's still
            // draining (kill → child exit → output EOF → read thread returns).
            // `writer` and `controller` then drop, closing conin and tearing down
            // the pseudoconsole — held until now so a still-running child isn't
            // terminated before we've drained its output.
            let _ = child.kill();
            if !read_finished {
                let _ = tokio::time::timeout(Duration::from_secs(2), read_handle).await;
            }

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
    //! 与 Windows 后端的一处刻意差异:收尾按**进程组**杀(`SIGHUP` 宽限
    //! 200ms 后 `SIGKILL`),不是只对顶层 pid 发信号——portable-pty 在
    //! `pre_exec` 里对子进程调了 `setsid()`,子进程 pid 即进程组号,只杀顶层
    //! pid 连坐不到 `nohup … &` 这类脱离父进程独立存活的孙进程。Windows 侧
    //! conpty-oxide 的 Job Object 是否已等价覆盖,没有 Windows 机器验证不了,
    //! 如实留白。
    //!
    //! 「首启等 2000ms 再发 `\r`」是启发式(赌 TUI 到这个点已加载完),不是
    //! 侦测到了就绪信号;更慢的网络/机器上可能不够,失效模式是 `\r` 送进一个
    //! 还没起来的输入框、等于没发。

    use std::io::{Read, Write};
    use std::time::Duration;

    use async_trait::async_trait;
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use tokio::sync::mpsc;

    use super::PtyBackend;
    use crate::interactive_cli::{LaunchPlan, PtyInput, SkillOutput};
    use crate::ExecError;

    pub struct UnixPtyBackend;

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
            // `CommandBuilder::new` 起步就整份拷贝了当前进程环境;清空后
            // `plan.env` 才真正成为子进程环境的唯一来源(见模块文档)。
            cmd.env_clear();
            for (k, v) in &plan.env {
                cmd.env(k, v);
            }
            cmd.cwd(&plan.cwd);

            let mut child = pair
                .slave
                .spawn_command(cmd)
                .map_err(|e| ExecError::Failed(format!("portable-pty spawn failed: {e}")))?;
            // 子进程自己持有一份从端;父进程这份必须释放,否则哪怕子进程已
            // 退出,只要还有一个从端引用活着,主端读侧永远等不到 EOF。
            drop(pair.slave);

            // 拿不到读/写端是早退路径:子进程已经真 spawn 起来了,先补一刀
            // `kill()` 再报错,不把它扔下不管。
            let mut reader = match pair.master.try_clone_reader() {
                Ok(r) => r,
                Err(e) => {
                    let _ = child.kill();
                    return Err(ExecError::Failed(format!(
                        "portable-pty clone reader failed: {e}"
                    )));
                }
            };
            let mut writer = match pair.master.take_writer() {
                Ok(w) => w,
                Err(e) => {
                    let _ = child.kill();
                    return Err(ExecError::Failed(format!(
                        "portable-pty take writer failed: {e}"
                    )));
                }
            };

            // 读循环(阻塞,spawn_blocking):drain 主端输出 → bytes_tx;读到
            // 0 字节或读错误(Unix 上从端全关后主端常读到 EIO)都当「子进程
            // 退出」处理。
            let read_tx = bytes_tx.clone();
            let read_handle = tokio::task::spawn_blocking(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if read_tx.send(buf[..n].to_vec()).is_err() {
                                break; // 调用方丢了接收端 —— 停止读
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // 读循环先结束时 `read_handle` 已在 `select!` 里被轮询到 Ready,
            // 收尾不能再 `.await` 一次(tokio panic「JoinHandle polled after
            // completion」;`bash -c 'echo pty-ok'` 这种立刻退出的子进程百分
            // 之百触发)。
            let mut read_finished = false;

            // 首启:位置 prompt 已以 argv 传给子进程,但没人替它按 Enter——等
            // TUI 加载完发一次 `\r`;续接(`submit_prompt == false`)不发。
            let submit_delay = tokio::time::sleep(Duration::from_millis(2000));
            tokio::pin!(submit_delay);
            let mut submitted = !plan.submit_prompt;

            tokio::pin!(read_handle);
            loop {
                tokio::select! {
                    _ = &mut read_handle => {
                        read_finished = true;
                        break;
                    }
                    input = input_rx.recv() => match input {
                        Some(PtyInput::Bytes(bytes)) => {
                            let _ = writer.write_all(&bytes);
                            let _ = writer.flush();
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
                        let _ = writer.write_all(b"\r");
                        let _ = writer.flush();
                    },
                }
            }

            // 收尾:按进程组杀 + `wait()` 收割退出状态(只 kill 不 wait 会留
            // 僵尸表项)。两者都是同步阻塞调用,挪进 spawn_blocking,不占
            // tokio 工作线程。`child` 是 `Box<dyn Child + Send + Sync>`,可以
            // 整个移进去。
            let _ = tokio::task::spawn_blocking(move || {
                if let Some(pid) = child.process_id() {
                    killpg_graceful(pid);
                }
                let _ = child.wait();
            })
            .await;

            // 这个 2s timeout 只是不再等 `JoinHandle`,并不能真的打断还卡在
            // 同步 `reader.read()` 上的 OS 线程——上面已 killpg 过,主端理应
            // 很快等到 EOF/EIO 让读线程自己退出。
            if !read_finished {
                let _ = tokio::time::timeout(Duration::from_secs(2), read_handle).await;
            }

            Ok(SkillOutput {
                completed: true,
                summary: "(pty session ended)".to_string(),
            })
        }
    }

    /// 按进程组「先礼后兵」收尾:先 `SIGHUP`(给子孙进程一个自行清理退出的
    /// 机会),宽限 200ms,再补 `SIGKILL` 兜底。
    ///
    /// `pid` 就是目标进程组号——portable-pty 的 unix 实现在
    /// `CommandBuilder::spawn_command` 的 `pre_exec` 里对子进程调了
    /// `setsid()`,子进程同时成为新会话与新进程组首领,`pid == pgid`。
    ///
    /// `killpg` 对已不存在的进程组返回 `ESRCH`——那是子进程已经经 EOF 路径
    /// 自然退出的正常情况,静默吞掉,与 `portable_pty::Child::kill` 对已退出
    /// 进程的幂等语义一致。用 `nix` 的安全封装而不是 `libc` 裸调用:bw-engine
    /// 整 crate `#![forbid(unsafe_code)]`,不为一个 syscall 开口子;`nix` 本就
    /// 在锁文件里(portable-pty 自身依赖)。
    fn killpg_graceful(pid: u32) {
        let pgid = Pid::from_raw(pid as i32);
        let _ = killpg(pgid, Signal::SIGHUP);
        std::thread::sleep(Duration::from_millis(200));
        let _ = killpg(pgid, Signal::SIGKILL);
    }
}
