//! PTY 烟测:不开界面,直接走 `InteractiveCliExecutor::run_skill_pty`(桌面壳
//! ▶跑 用的同一条路径),在当前平台的 PTY 后端里起一个最小的 shell 命令
//! (macOS/Linux 是 `bash -c 'echo pty-ok'`,Windows 是 `cmd /C echo pty-ok`),
//! 读回字节,核对里面确实有 `pty-ok`。
//!
//! 用途:证明「内嵌终端在这台机器上能真起子进程、真读到输出、真收尾」——
//! 这是主环在 macOS 上能跑的前提。不碰 `claude`,不碰网关,可随时重跑。
//!
//! ```bash
//! cargo run -p v4-engine --example pty_smoke              # 起 bash 读回 pty-ok
//! cargo run -p v4-engine --example pty_smoke -- --teardown # 收尾:丢输入端 → 杀整个进程组
//! cargo run -p v4-engine --example pty_smoke -- --abort    # 中止:abort() 丢弃 future → 子进程也得死
//! ```
//!
//! `--teardown` 场景模拟「用户关掉运行、App 丢掉输入端」:子进程是
//! `bash -c 'nohup sleep 30 & sleep 30'`(带一个脱离父进程的孙进程),
//! 500ms 后丢输入端,断言后端 5s 内返回、且孙进程 `sleep 30` 也被连坐——
//! 只杀顶层 pid 的实现会让它活到自然结束。
//!
//! `--abort` 场景模拟 bw-app `cancel_run` 的真实做法——对跑着后端的 tokio
//! 任务调 `JoinHandle::abort()`,future 在 `select!` 中途被丢弃、正常收尾代码
//! 根本不会执行。断言 3s 内顶层与孙进程都没了(靠 `ChildGuard` 的 `Drop`)。
//! 2026-08-17 评审抓出的真 bug:改前 macOS 上中止一次就漏一个孤儿 `claude`。
//!
//! **`--teardown` 与 `--abort` 只在 macOS/Linux 上有实现**:它们靠 `nohup` 造一
//! 个脱离出去的孙进程、靠 `pgrep` 读回残留,Windows 上这两样都没有对应写法,还
//! 没搬过去。在 Windows 上跑这两个开关会如实说一句「没实现」并以 2 退出 ——
//! **不拿一个空跑冒充通过**。基本场景(不带开关)两个平台都能跑。
//!
//! 退出码:0 = 通过;1 = 没读到 / 后端报错 / 收尾超时或孙进程残留;
//! 2 = 这个场景在本平台还没实现。stderr 打 `[PTY_SMOKE]` 行,方便脚本抓。

use std::collections::HashMap;
// 这两个只被收尾 / 中止那两个场景用,而它们只有 macOS/Linux 有实现。
#[cfg(not(windows))]
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use v4_engine::interactive_cli::{
    InteractiveCliExecutor, InteractiveExecutor, LaunchPlan, PtyInput,
};
use v4_engine::RunCtx;

#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--teardown") {
        teardown_scenario().await;
        return;
    }
    if std::env::args().any(|a| a == "--abort") {
        abort_scenario().await;
        return;
    }
    let plan = shell_plan("echo pty-ok");
    let ctx = nil_ctx();

    let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();
    // 桌面壳 attach 时会先发一次尺寸;这里照做,顺便证明 resize 通道不炸。
    let _ = input_tx.send(PtyInput::Resize {
        cols: 100,
        rows: 30,
    });

    let executor = InteractiveCliExecutor::new();
    let run = executor.run_skill_pty(&plan, &ctx, bytes_tx, input_rx);

    let collect = async {
        let mut all = Vec::new();
        while let Some(chunk) = bytes_rx.recv().await {
            all.extend_from_slice(&chunk);
        }
        all
    };

    let (outcome, bytes) = tokio::join!(run, collect);
    // `input_tx` 活到这里才 drop,免得后端在子进程退出前先看到输入端关闭。
    drop(input_tx);

    let text = String::from_utf8_lossy(&bytes);
    match outcome {
        Ok(out) => eprintln!(
            "[PTY_SMOKE] backend returned completed={} summary={:?} bytes={}",
            out.completed,
            out.summary,
            bytes.len()
        ),
        Err(e) => {
            eprintln!("[PTY_SMOKE] backend error: {e}");
            std::process::exit(1);
        }
    }
    if text.contains("pty-ok") {
        eprintln!("[PTY_SMOKE] OK — read back {:?}", text.trim());
    } else {
        eprintln!("[PTY_SMOKE] FAIL — expected `pty-ok` in output, got {text:?}");
        std::process::exit(1);
    }
}

/// 把一段 shell 命令包成启动计划。与 `build_startup_plan` 同款:整份环境快照当
/// 子进程环境(这里不需要剥嵌套会话变量——跑的是 shell,不是 claude);不是
/// claude 首启,不需要自动按 Enter。
///
/// **按平台选 shell**:Windows 默认没有 `bash`,写死它等于这个工具在 Windows 上
/// 开箱就跑不了 —— 而它是不碰 claude、不碰网关就能验 PTY 后端的唯一手段。
fn shell_plan(script: &str) -> LaunchPlan {
    #[cfg(windows)]
    let (binary, args) = ("cmd", vec!["/C".to_string(), script.to_string()]);
    #[cfg(not(windows))]
    let (binary, args) = ("bash", vec!["-c".to_string(), script.to_string()]);
    let env: HashMap<String, String> = std::env::vars().collect();
    LaunchPlan {
        binary: binary.to_string(),
        args,
        env,
        cwd: std::env::current_dir().expect("cwd"),
        submit_prompt: false,
    }
}

/// 烟测不属于任何项目/工作流,`RunCtx` 全 nil。
fn nil_ctx() -> RunCtx {
    RunCtx {
        project: uuid::Uuid::nil(),
        workflow: uuid::Uuid::nil(),
    }
}

// ─────────── 收尾 / 中止两个场景:只有 macOS/Linux 有实现 ───────────
//
// 它们靠 `nohup` 造一个脱离父进程的孙进程、靠 `pgrep -f <标记>` 读回残留 ——
// Windows 上这两样都没有对应写法,还没搬过去。**不拿一个空跑冒充通过**:如实
// 说一句就以 2 退出,既不算通过也不算失败。

#[cfg(windows)]
async fn teardown_scenario() {
    unimplemented_on_windows("--teardown");
}

#[cfg(windows)]
async fn abort_scenario() {
    unimplemented_on_windows("--abort");
}

#[cfg(windows)]
fn unimplemented_on_windows(flag: &str) -> ! {
    eprintln!(
        "[PTY_SMOKE] UNIMPLEMENTED — {flag} 靠 nohup 造孙进程、靠 pgrep 读回残留,Windows 上还没有对应写法。基本场景(不带开关)照常可跑。"
    );
    std::process::exit(2);
}

#[cfg(not(windows))]
/// 「顶层 + 一个 nohup 脱离出去的孙进程」都睡在一个独一无二的时长上,事后
/// 用 `pgrep -f <marker>` 读回有没有残留(读回为证,不信返回值)。
fn marked_sleep_script(marker: &str) -> String {
    format!("nohup {marker} >/dev/null 2>&1 & {marker}")
}

#[cfg(not(windows))]
fn survivors_of(marker: &str) -> String {
    std::process::Command::new("pgrep")
        .arg("-f")
        .arg(marker)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(not(windows))]
/// 收尾场景:子进程还活着时 App 丢掉输入端 → 后端必须按进程组把子孙一起
/// 杀掉并及时返回。
async fn teardown_scenario() {
    let marker = format!("sleep 30.{}", std::process::id());
    let plan = shell_plan(&marked_sleep_script(&marker));
    let ctx = nil_ctx();
    let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();
    let executor = InteractiveCliExecutor::new();

    let drain = tokio::spawn(async move { while bytes_rx.recv().await.is_some() {} });
    let dropper = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        drop(input_tx);
    });
    let started = Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        executor.run_skill_pty(&plan, &ctx, bytes_tx, input_rx),
    )
    .await;
    let _ = dropper.await;
    let _ = drain.await;
    match outcome {
        Ok(Ok(out)) => eprintln!(
            "[PTY_SMOKE] teardown returned in {:?} completed={}",
            started.elapsed(),
            out.completed
        ),
        Ok(Err(e)) => {
            eprintln!("[PTY_SMOKE] teardown backend error: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("[PTY_SMOKE] FAIL — teardown did not return within 5s");
            std::process::exit(1);
        }
    }
    // 读回:标记 sleep 不能还活着(顶层 + nohup 孙进程都得没了)。
    let survivors = survivors_of(&marker);
    if survivors.is_empty() {
        eprintln!("[PTY_SMOKE] OK — no `{marker}` survivors (process group reaped)");
    } else {
        eprintln!("[PTY_SMOKE] FAIL — survivors after teardown: {survivors}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
/// 中止场景:和 bw-app `cancel_run` 一模一样地 `abort()` 掉跑着后端的任务。
/// 输入端故意一直握着——要证明的是「future 被丢弃」这一件事就足以收尾,
/// 不靠输入端关闭那条正常路径。
async fn abort_scenario() {
    let marker = format!("sleep 31.{}", std::process::id());
    let plan = shell_plan(&marked_sleep_script(&marker));
    let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();

    let drain = tokio::spawn(async move { while bytes_rx.recv().await.is_some() {} });
    let handle = tokio::spawn(async move {
        let executor = InteractiveCliExecutor::new();
        let ctx = nil_ctx();
        executor
            .run_skill_pty(&plan, &ctx, bytes_tx, input_rx)
            .await
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    let alive_before = survivors_of(&marker);
    if alive_before.is_empty() {
        eprintln!("[PTY_SMOKE] FAIL — `{marker}` never started (nothing to abort)");
        std::process::exit(1);
    }
    let started = Instant::now();
    handle.abort();
    // `handle.await` 得到 JoinError::Cancelled —— 正是 cancel_run 看到的样子。
    let _ = handle.await;
    // 收尾在独立线程上进行,最多等 3s(宽限本身 ≤200ms)。
    let deadline = started + Duration::from_secs(3);
    let mut survivors = survivors_of(&marker);
    while !survivors.is_empty() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
        survivors = survivors_of(&marker);
    }
    // 读线程随主端 EOF/EIO 退出后 `bytes_tx` 才释放、drain 才结束——也给它
    // 一个上限,读线程没退出同样算失败(下面 survivors 非空会一起暴露)。
    let _ = tokio::time::timeout(Duration::from_secs(3), drain).await;
    drop(input_tx);
    if survivors.is_empty() {
        eprintln!(
            "[PTY_SMOKE] OK — abort() reaped `{marker}` (was pid(s) {}) in {:?}",
            alive_before.replace('\n', ","),
            started.elapsed()
        );
    } else {
        eprintln!("[PTY_SMOKE] FAIL — survivors after abort(): {survivors}");
        std::process::exit(1);
    }
}
