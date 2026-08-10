//! `pty_smoke` — PTY 后端接缝的本机烟测(next 切片三B)。自我标注:这是
//! 流程演示,不是单元测试(本仓核心纪律不写/不留单元测试,靠 headless
//! 指挥器读回)。
//!
//! **不依赖 claude/网关**:用 [`bw_engine::pty_backend::active`] 选出的当前
//! 平台 PTY 后端起一个**普通 shell 进程**(`bash -c 'echo pty-ok'`),证明
//! 「起进程 → 经 PTY 双向倒腾字节 → 干净收尾」这条链路本体真的能跑——这是
//! 裁决 #7'(macOS 现实)要求的最小验收:不补 Unix 后端,这条链路在本机为
//! 零;补上以后,这个烟测就是它「真能工作」的证据,和 claude CLI 是否装了
//! 无关。
//!
//! 跑法:`cd next && cargo run -p bw-engine --example pty_smoke`
//! 退出码 0、末行 `PTY_SMOKE_OK`、且读回的字节里含 `pty-ok` = 通过。

use std::process::ExitCode;
use std::time::Duration;

use bw_engine::interactive_cli::{LaunchPlan, PtyInput};
use bw_engine::pty_backend::{self, PtyBackend};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> ExitCode {
    println!("== PTY 后端烟测(普通 shell 进程,不依赖 claude)==");

    let workspace = std::env::temp_dir().join(format!("bw-pty-smoke-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!(
            "ASSERT FAILED: 无法建临时工作区 {}: {e}",
            workspace.display()
        );
        return ExitCode::FAILURE;
    }

    // 跑 `bash -c 'echo pty-ok'`——一次性输出、立刻退出,足够证明双向字节
    // 链路能工作,不需要真的交互。
    let plan = LaunchPlan {
        binary: "bash".to_string(),
        args: vec!["-c".to_string(), "echo pty-ok".to_string()],
        env: Default::default(),
        cwd: workspace.clone(),
        submit_prompt: false,
    };

    let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (_input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();

    let backend = pty_backend::active();
    // 15s 挂钟兜底——烟测本身不该卡死;真卡住了就是后端有 bug,不是本脚本
    // 该吞掉的东西。
    let run_result = tokio::time::timeout(
        Duration::from_secs(15),
        backend.run("bash", &plan, bytes_tx, input_rx),
    )
    .await;

    let _ = std::fs::remove_dir_all(&workspace);

    let output = match run_result {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            eprintln!("ASSERT FAILED: PTY 后端返回错误: {e}");
            return ExitCode::FAILURE;
        }
        Err(_) => {
            eprintln!("ASSERT FAILED: PTY 后端 15s 内未收尾(疑似卡死)");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "run() 返回: completed={} summary={:?}",
        output.completed, output.summary
    );

    let mut collected = Vec::new();
    while let Ok(chunk) = bytes_rx.try_recv() {
        collected.extend_from_slice(&chunk);
    }
    let text = String::from_utf8_lossy(&collected);
    println!("读回字节({} 字节): {:?}", collected.len(), text);
    println!();

    let mut ok = true;
    if !text.contains("pty-ok") {
        eprintln!("ASSERT FAILED: 读回输出应含 \"pty-ok\",实得: {text:?}");
        ok = false;
    }
    if !output.completed {
        eprintln!("ASSERT FAILED: SkillOutput.completed 应为 true");
        ok = false;
    }

    if ok {
        println!("PTY_SMOKE_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("PTY_SMOKE_FAILED");
        ExitCode::FAILURE
    }
}
