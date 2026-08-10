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

// 评审顺手一并 #8:非 unix/非 windows 目标(例如 wasm32)编译到这里,
// 之前会因为 `active()` 函数体空着落到「类型不匹配」这种指向不明的报错;
// 显式 `compile_error!` 把「PTY 后端本来就只做了两家」这件事在编译期挑
// 明,不留一个含糊的兜底分支让人以为漏配置了什么。
#[cfg(not(any(unix, windows)))]
compile_error!("PTY 后端仅支持 unix/windows");

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
            //
            // **已知缺口,不在本片修(登记日 2026-08-10,详情见
            // plan/23-opc-stitching-rebuild.md §10 第一条)**:机制一句话
            // ——子进程正常退出(读循环先于 input_rx 分支完成,是最常见
            // 的收尾路径)时,`read_handle` 已经在上面的 `select!` 里被
            // 轮询到 `Ready` 过一次,下面这行无条件再 `.await` 同一个
            // `JoinHandle` 会 panic「JoinHandle polled after completion」
            // ——Unix 侧(`unix` 模块)用 `read_finished` 标志位修过同一
            // 个问题,这里零改写约束下原样保留,需要 Windows 机器验证真
            // 机行为再修。
            let _ = child.kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), read_handle).await;

            Ok(SkillOutput {
                completed: true,
                summary: "(pty session ended)".to_string(),
                // next 切片三-2 修(I5):这条收尾只 `child.kill()`,从没
                // `.wait()` 过——`conpty_oxide::blocking::Child` 有没有等价
                // 的退出码读法,零改写约束下（本函数体整段搬自 v1，不擅
                // 改）没有去核实、也不去加一次新的 `wait()` 调用。如实
                // `None`，不假装量出来一个（见 `SkillOutput::exit_code`
                // 字段文档）。
                exit_code: None,
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
    //! 与 Windows 实现的行为语义对照(切片三-1 修如实分列,不再笼统写
    //! 「对齐」——评审指出「对齐」掩盖了下面这条真实差异):
    //!
    //! **对齐点**:`run` 立刻起好可用的输入/输出两端;子进程退出(读到
    //! EOF/读错误)才收尾;收尾杀进程幂等(子进程/进程组已经不在时再杀
    //! 是 no-op——`libc::killpg`/`portable_pty::Child::kill` 对已退出的
    //! 目标返回 `ESRCH`/无害错误,这里统一吞掉不当失败处理)。
    //!
    //! **差异点(切片三-1 修的真 bug,Windows 侧未跟进)**:收尾杀进程
    //! 从「只对顶层子进程单个 pid 发信号」改成了「按进程组 `killpg`,
    //! `SIGHUP` 宽限 200ms 后 `SIGKILL`」,连坐 `nohup ... &` 这类脱离父
    //! 进程独立存活的孙进程(见 `unix::killpg_graceful` 文档与
    //! `unix` 模块收尾段落)。Windows 那份整段搬运的实现里,收尾仍是原
    //! 逻辑的 `child.kill()`(零改写约束,本片不碰它的收尾逻辑本身,只
    //! 加了一句指向已知缺口登记的标记注释,见 `windows` 模块);它的孙
    //! 进程是否也需要连坐、conpty-oxide 的 Job Object 机制是否已经等价
    //! 覆盖,本片没有 Windows 机器验证不了,如实留白,不假装两边现在是
    //! 同一种机制。

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

            // 下面两步失败是早退路径(评审顺手一并 #6):子进程已经真 spawn
            // 起来了,拿不到读/写端不能就这么把它扔下不管——先补一刀
            // `child.kill()` 再报错退出,不指望调用方替这条命运负责。
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

            // 收尾(评审 must-fix #1 + #2):按进程组真杀 + 阻塞调用挪进
            // spawn_blocking。`writer`/`pair.master` 随函数返回一并 drop,
            // 关闭主端。
            //
            // 为什么要按进程组杀:`portable-pty` 的 `Child::kill()`(见上面
            // 旧版本这里调的那个)只对 `child` 这一个顶层 pid 发信号——但
            // `CommandBuilder::spawn_command` 在 `pre_exec` 里对子进程调了
            // `setsid()`(portable-pty 0.9.0 `unix.rs`),子进程同时成为新
            // 会话与新进程组的首领,`child` 的 pid 正好就是这个进程组的
            // pgid。只杀顶层 pid 连坐不到它 fork 出来的孙进程——评审给的
            // 复现法 `bash -c "nohup sleep 25 & echo started"` 里,`nohup`
            // 的孙进程 `sleep` 会脱离父进程独立存活,继续跑到自然结束,是
            // 需要真杀掉的真 bug。改用 `killpg_graceful`(下方定义)对整个
            // 进程组先 `SIGHUP` 宽限 200ms 再 `SIGKILL`,连坐子孙进程。
            //
            // 为什么要挪进 spawn_blocking:`killpg_graceful` 内含一次
            // `std::thread::sleep(200ms)`,`child.wait()` 是阻塞系统调用
            // (收割退出状态、避免僵尸进程表项常驻——kill 只发信号,不 wait
            // 内核不会真的把这条记录清掉)——两个都是同步阻塞调用,不能直
            // 接摆在 async fn 里跑,会占住 tokio 运行时的工作线程;`child`
            // 本身(`Box<dyn portable_pty::Child + Send + Sync>`)满足
            // `spawn_blocking` 要求的 `'static + Send`,直接把它连同两个
            // 调用一起移进去。
            // next 切片三-2 修(I5):`child.wait()` 的返回值以前被
            // `let _ = ...` 直接扔了——退出码就在手边,顺手接上,不是新
            // 观测手段。`portable_pty::ExitStatus::exit_code()` 拿 `u32`,
            // 这里转 `i32` 存进 `SkillOutput.exit_code`(与
            // `std::process::ExitStatus::code()` 的 `i32` 同型,方便调用方
            // 统一处理)。`child.wait()` 失败(比如已经被上面的
            // killpg_graceful 连坐、内核状态竞态导致读不到)时如实 `None`,
            // 不编一个假的 0。
            let exit_code = tokio::task::spawn_blocking(move || {
                if let Some(pid) = child.process_id() {
                    killpg_graceful(pid);
                }
                child.wait().ok().map(|status| status.exit_code() as i32)
            })
            .await
            .expect("pty 收尾的 spawn_blocking 任务不应 panic");

            // 已知继承问题(评审顺手一并 #7,不修,如实记录):这里的 2s
            // timeout 只是不再 `.await` 这个 `JoinHandle`,并不能真的打断
            // spawn_blocking 里那个还卡在同步 `reader.read()` 调用上的 OS
            // 线程——上面已经 killpg 过,主端理应很快等到 EOF/EIO 让读线
            // 程自己退出,但如果内核迟迟不给(理论上限,不是本片观察到的
            // 实际现象),这个线程会一直占着直到它的 `read()` 真的被唤醒
            // 返回,不会被这个 timeout 强制打断。Windows 那份整段搬运的实
            // 现里,收尾处同一形状的 `tokio::time::timeout(..., read_handle)
            // .await` 有一模一样的继承问题(同样只是不等,不是真取消)——
            // 两边都不修,只如实标注,和 `pty_backend.rs` 顶部模块文档配对。
            if !read_finished {
                let _ = tokio::time::timeout(Duration::from_secs(2), read_handle).await;
            }

            Ok(SkillOutput {
                completed: true,
                summary: "(pty session ended)".to_string(),
                exit_code,
            })
        }
    }

    /// 按进程组做「先礼后兵」式收尾杀进程(评审 must-fix #1):先
    /// `SIGHUP`(挂断,给孙进程一个借机自行清理退出的机会),等一段宽限
    /// 期,再补一记 `SIGKILL`(强杀,兜底不理会 `SIGHUP` 的顽固进程)。
    ///
    /// `pid` 就是目标进程组号——`portable-pty` 的 unix 实现在
    /// `CommandBuilder::spawn_command` 的 `pre_exec` 里对每个子进程调用了
    /// `setsid()`(见 portable-pty 0.9.0 `src/unix.rs`),使子进程同时成为
    /// 新会话与新进程组的首领,故 `pid == pgid`,直接拿子进程 pid 当
    /// `killpg` 的目标组号是对的,不需要另外查 `getpgid`。
    ///
    /// 宽限期取 200ms:比 0(纯 `SIGKILL` 连坐,来不及跑清理逻辑的子进程
    /// 没有机会)长,又不会让收尾路径等太久——量级对齐 `portable-pty`
    /// 自身 `std::process::Child::kill`(单 pid 版)的宽限设计(5×50ms,
    /// 见其 `src/lib.rs`)。
    ///
    /// 选 `libc` 不选 `nix`:见 `Cargo.toml` `[target.'cfg(unix)'.dependencies]`
    /// 的选型说明——两者都已经在锁文件里(`portable-pty` 自身就依赖
    /// 它俩),`libc` 的裸 extern 调用够用,不需要 `nix` 的 `Signal`/`Pid`
    /// 包装类型。
    ///
    /// `killpg` 对已经不存在的进程组返回 `ESRCH`(`-1` + `errno`)——这是
    /// 子进程已经通过 EOF 路径自然退出的正常情况,不是错误,这里静默吞掉
    /// 不处理(与 `portable_pty::Child::kill` 对已退出进程的幂等语义
    /// 一致,见模块顶部文档「对齐点」)。
    fn killpg_graceful(pid: u32) {
        let pgid = pid as libc::pid_t;
        unsafe {
            libc::killpg(pgid, libc::SIGHUP);
        }
        std::thread::sleep(Duration::from_millis(200));
        unsafe {
            libc::killpg(pgid, libc::SIGKILL);
        }
    }
}
