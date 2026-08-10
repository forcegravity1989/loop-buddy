//! PTY 平台接缝(next 切片三B,design-s3-agentcli.md §7/§9 提取)。
//!
//! `InteractiveCliExecutor::run_skill_pty`(`interactive_cli.rs`)原来整个
//! 函数体挂 `#[cfg(windows)]`——Windows 用 conpty-oxide,非 Windows 没有
//! override,直接落到 trait 默认实现「PTY not supported」。本模块把「怎么起
//! 一个 PTY 子进程、怎么双向倒腾字节、怎么改尺寸、怎么收尾杀进程」这层平台
//! 分叉从函数体里剥出来,变成同一个 trait 的两份平台实现:
//!
//! - [`windows::WindowsPtyBackend`]:conpty-oxide 那段函数体**整段搬**过来,
//!   一行逻辑不改。唯一的必要调整是「解析 claude 可执行路径」——原来直接读
//!   `InteractiveCliExecutor::claude_binary` 字段,现在改成读调用方已经解析
//!   好、当参数传进来的 `binary: &str`(接缝提取的天然结果:同一个值,只是
//!   传递方式从字段访问变成参数,不是逻辑变化)。
//! - [`unix::UnixPtyBackend`]:本片唯一新写项,用 `portable-pty` 补上
//!   macOS/Linux 缺的那一半(主控裁决 #2:本机 macOS 是部署机,必须能真
//!   跑)。范围钉在「spawn / 字节双向 / 尺寸 / kill」的最小集,不含 Windows
//!   实现里那段 claude 专属的自动提交逻辑——见该模块文档的如实说明。
//!
//! 调用方只认 [`PtyBackend`] 这一个 trait;[`active`] 按 cfg 选一份实现,
//! `interactive_cli.rs` 里不再出现任何 `#[cfg(windows)]`/`#[cfg(unix)]`。
//!
//! **本片实测揪出一个疑似上游 bug,如实记在这里,不动 Windows 迁移件**:
//! 收尾那段「`tokio::select!` 循环里 `_ = &mut read_handle => break`,循环
//! 外再无条件 `let _ = timeout(..., read_handle).await`」的写法,一旦读循环
//! 那一支先完成(子进程正常退出、读到 EOF 是最常见的收尾路径),
//! `read_handle`(`tokio::task::JoinHandle`)已经在 `select!` 里被轮询到
//! `Ready` 过一次;循环外再 `.await` 同一个 `read_handle` 会 panic
//! 「JoinHandle polled after completion」——`pty_smoke` 烟测跑
//! `bash -c 'echo pty-ok'`(子进程几乎立刻退出)百分之百复现。这段写法在
//! Windows 那份**整段搬运**的实现里原样保留(零逻辑改写的约束不允许在那
//! 里修);[`unix::UnixPtyBackend`] 是本片新写代码,不受该约束,已经用
//! `read_finished` 标志位避开重复 poll。Windows 侧这个隐患留给有 Windows
//! 机器的人验证并修——本片证不了 Windows 真机行为。

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::interactive_cli::{LaunchPlan, PtyInput, SkillOutput};
use crate::ExecError;

/// PTY 平台接缝。**只管「起进程、双向倒腾字节、改尺寸、收尾杀进程」这四件
/// 事**——不含「怎么判定活干完了」(那是更上层
/// [`crate::interactive_cli::InteractiveExecutor`] 的事;这里的
/// `SkillOutput` 只表达「PTY 子进程退出了」这一件事实,`completed` 恒
/// `true`,不代表「活干成了」)。
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

/// 按当前编译目标选可用的 PTY 后端。两个 `#[cfg(...)]` 互斥,每个平台的
/// 构建里只有一支存在——用 `impl Trait` 返回而不是 `Box<dyn PtyBackend>`,
/// 两个后端都是零字段的单元结构体,不需要为了统一签名多付一次堆分配。
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
    //! Windows PTY 后端:conpty-oxide。`run` 方法体**整段搬自**切片三B 提取
    //! 前 `InteractiveCliExecutor::run_skill_pty` 的 `#[cfg(windows)]`
    //! override(v1 Issue2 W2 §9),零逻辑改写——唯一改动是最外层不再从
    //! `self.claude_binary` 字段读可执行路径,改成读调用方(`interactive_cli.rs`
    //! 里的 `run_skill_pty` 包装函数)已经解析好、当 `binary` 参数传进来的值,
    //! 详见本模块顶部文档。

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
        /// V1 Issue2 W2 (§9): spawn `claude` in a PTY (conpty-oxide on Windows)
        /// and stream bytes. Replaces the portable-pty 0.9.0 impl whose ConPTY
        /// backend doesn't deliver child stdout to the reader (see §9).
        ///
        /// Flow (conpty-oxide `blocking::Command` + `Session::into_parts`):
        ///  - `Command::new(binary).args().env().current_dir().spawn()` → `Session`
        ///  - `Session::into_parts()` → `{ child, output, input, controller }`
        ///  - `output` (OwnedReadHalf: std::io::Read) → spawn_blocking read loop →
        ///    `bytes_tx` (drains ConPTY output so the pipe can't fill —
        ///    `Child::wait` deadlocks otherwise, per conpty-oxide module docs)
        ///  - `input` (OwnedWriteHalf: std::io::Write) ← `PtyInput::Bytes`
        ///  - `controller.resize(Size)` ← `PtyInput::Resize`
        ///  - §9.5: on first run (`plan.submit_prompt`), send `\r` after a brief
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

            // §9.5: claude interactive positional argv doesn't auto-submit in
            // buddy's GLM-gateway environment — the TUI starts and waits for
            // Enter. On first run (submit_prompt), wait briefly for the TUI to
            // load, then send `\r` to submit the positional skill body. The read
            // loop drains concurrently so the pipe can't fill during this wait.
            // Resume (submit_prompt=false) has no positional — nothing to submit.
            let submit_delay = tokio::time::sleep(Duration::from_millis(2000));
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
            let _ = tokio::time::timeout(Duration::from_secs(2), read_handle).await;

            Ok(SkillOutput {
                completed: true,
                summary: "(pty session ended)".to_string(),
            })
        }
    }
}

#[cfg(unix)]
pub mod unix {
    //! Unix PTY 后端 —— 本片(切片三B)唯一新写项。用 `portable-pty`(选型
    //! 优先于自建,design-s3-agentcli.md §7.4 / 主控裁决 #2)补上 macOS/
    //! Linux 缺的那一半:起 PTY 子进程、双向倒腾字节、resize、kill 的最小集。
    //!
    //! **与 Windows 实现的一处刻意不对齐,如实记在这里**:Windows 实现里那
    //! 段「首次运行等 TUI 加载完,自动发 `\r` 提交位置 prompt」的逻辑
    //! (`submit_delay`/`submitted`)是 buddy 在 GLM 网关环境下测出来的
    //! claude 交互式 TUI 不自动提交的补丁,不是 PTY 后端本身该有的语义。
    //! design 明确把 Unix 后端的范围钉在「spawn / 字节双向 / 尺寸 / kill」
    //! 的最小集,不含这条——这里不悄悄补上,免得下一个人以为漏了没做;真
    //! 要在 macOS 上验证是否也需要这个补丁,得先有一次真实交互式 claude
    //! 会话观察(留给切片三 C/D 接线后)。
    //!
    //! 行为语义对齐 Windows 实现:`run` 立刻起好可用的输入/输出两端;子
    //! 进程退出(读到 EOF/读错误)才收尾;`kill` 幂等(子进程已经退出时
    //! 再 kill 是 no-op,`portable_pty::Child::kill` 对已退出进程返回
    //! `Ok`/无害错误,这里统一吞掉不当失败处理)。

    use std::io::{Read, Write};
    use std::time::Duration;

    use async_trait::async_trait;
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
            // 初始尺寸给个常见的终端默认值——真实 resize 由调用方经
            // `PtyInput::Resize` 补(桌面壳 attach 时会立刻发一次,见
            // `terminal_manager.rs` 的 `attach`)。
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
            for (k, v) in &plan.env {
                cmd.env(k, v);
            }
            cmd.cwd(&plan.cwd);

            let mut child = pair
                .slave
                .spawn_command(cmd)
                .map_err(|e| ExecError::Failed(format!("portable-pty spawn failed: {e}")))?;
            // 子进程自己持有一份从端引用;父进程这份用不上了。不释放的话主端
            // 读侧永远等不到 EOF(哪怕子进程已经退出,只要还有一个从端引用
            // 活着,内核就不会给主端发 EOF)。
            drop(pair.slave);

            let mut reader = pair
                .master
                .try_clone_reader()
                .map_err(|e| ExecError::Failed(format!("portable-pty clone reader failed: {e}")))?;
            let mut writer = pair
                .master
                .take_writer()
                .map_err(|e| ExecError::Failed(format!("portable-pty take writer failed: {e}")))?;

            // 读循环(阻塞,spawn_blocking):行为对齐 Windows 实现——drain
            // 主端输出 → bytes_tx;读到 0 字节或读错误(常见于 Unix 上从端
            // 全关后主端读到 EIO)都当「子进程退出」处理,不区分对待。
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

            // `read_finished`:哪个分支 break 出循环的,决定收尾时还能不能
            // 再 `.await` 一次 `read_handle`——**这是本片实测揪出的一个真
            // bug,记在这里免得下一个人踩第二次**:tokio 的 `JoinHandle` 一旦
            // 被 `select!` 轮询到 `Ready`(读循环先结束这一支),这个
            // `JoinHandle` 就已经「消费」过了,收尾时如果不分支、无条件再
            // `.await` 同一个 `read_handle`(Windows 那段整段搬来的实现正是
            // 这么写的),会直接 panic「JoinHandle polled after completion」
            // ——`pty_smoke` 烟测跑 `bash -c 'echo pty-ok'` 时百分之百复现
            // (子进程几乎立刻退出,读循环必然先于 `input_rx` 那支完成)。
            // Unix 后端是本片新写代码,不受「Windows 函数体零改写」约束,这
            // 里按正确写法处理;Windows 迁移件保留原样不动,已把同一个隐患
            // 记进本文件顶部注释与任务报告的 concerns。
            let mut read_finished = false;
            tokio::pin!(read_handle);
            loop {
                tokio::select! {
                    // 读循环结束(子进程退出 / EOF / 调用方丢了 bytes_rx)。
                    _ = &mut read_handle => {
                        read_finished = true;
                        break;
                    }
                    // 调用方送来的键盘字节或 resize 请求。
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
                }
            }

            // 收尾:杀子进程(幂等——已经因 EOF 自然退出时这里是 no-op),
            // 回收避免僵尸进程,再等读线程收尾(仅当读循环还没在上面的
            // select! 里被消费完——见 `read_finished` 注释)。`writer`/
            // `pair.master` 随函数返回一并 drop,关闭主端。
            let _ = child.kill();
            let _ = child.wait();
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
