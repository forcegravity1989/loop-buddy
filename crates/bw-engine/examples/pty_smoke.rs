//! PTY 烟测:不开界面,直接走 `InteractiveCliExecutor::run_skill_pty`(桌面壳
//! ▶跑 用的同一条路径),在当前平台的 PTY 后端里起 `bash -c 'echo pty-ok'`,
//! 读回字节,核对里面确实有 `pty-ok`。
//!
//! 用途:证明「内嵌终端在这台机器上能真起子进程、真读到输出、真收尾」——
//! 这是主环在 macOS 上能跑的前提。不碰 `claude`,不碰网关,可随时重跑。
//!
//! ```bash
//! cargo run -p bw-engine --example pty_smoke              # 起 bash 读回 pty-ok
//! cargo run -p bw-engine --example pty_smoke -- --teardown # 收尾:杀整个进程组
//! ```
//!
//! `--teardown` 场景模拟「用户关掉运行、App 丢掉输入端」:子进程是
//! `bash -c 'nohup sleep 30 & sleep 30'`(带一个脱离父进程的孙进程),
//! 500ms 后丢输入端,断言后端 5s 内返回、且孙进程 `sleep 30` 也被连坐——
//! 只杀顶层 pid 的实现会让它活到自然结束。
//!
//! 退出码:0 = 通过;1 = 没读到 / 后端报错 / 收尾超时或孙进程残留。stderr
//! 打 `[PTY_SMOKE]` 行,方便脚本抓。

use std::collections::HashMap;

use bw_engine::interactive_cli::{
    InteractiveCliExecutor, InteractiveExecutor, LaunchPlan, PtyInput,
};
use bw_engine::RunCtx;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--teardown") {
        teardown_scenario().await;
        return;
    }
    let cwd = std::env::current_dir().expect("cwd");
    // 与 build_startup_plan 同款:整份环境快照当子进程环境(这里不需要
    // 剥嵌套会话变量——跑的是 bash,不是 claude)。
    let env: HashMap<String, String> = std::env::vars().collect();
    let plan = LaunchPlan {
        binary: "bash".to_string(),
        args: vec!["-c".to_string(), "echo pty-ok".to_string()],
        env,
        cwd,
        // 不是 claude 首启,不需要自动按 Enter。
        submit_prompt: false,
    };
    let ctx = RunCtx {
        project: bw_core::ProjectId::nil(),
        workflow: bw_core::WorkflowId::nil(),
    };

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

/// 收尾场景:子进程还活着时 App 丢掉输入端 → 后端必须按进程组把子孙一起
/// 杀掉并及时返回。用一个独一无二的 sleep 时长当标记,事后 `pgrep -f` 核对
/// 没有残留(读回为证,不信返回值)。
async fn teardown_scenario() {
    let marker = format!("sleep 30.{}", std::process::id());
    let cwd = std::env::current_dir().expect("cwd");
    let env: HashMap<String, String> = std::env::vars().collect();
    let plan = LaunchPlan {
        binary: "bash".to_string(),
        args: vec![
            "-c".to_string(),
            format!("nohup {marker} >/dev/null 2>&1 & {marker}"),
        ],
        env,
        cwd,
        submit_prompt: false,
    };
    let ctx = RunCtx {
        project: bw_core::ProjectId::nil(),
        workflow: bw_core::WorkflowId::nil(),
    };
    let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();
    let executor = InteractiveCliExecutor::new();

    let drain = tokio::spawn(async move { while bytes_rx.recv().await.is_some() {} });
    let dropper = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        drop(input_tx);
    });
    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
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
    let survivors = std::process::Command::new("pgrep")
        .arg("-f")
        .arg(&marker)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if survivors.is_empty() {
        eprintln!("[PTY_SMOKE] OK — no `{marker}` survivors (process group reaped)");
    } else {
        eprintln!("[PTY_SMOKE] FAIL — survivors after teardown: {survivors}");
        std::process::exit(1);
    }
}
