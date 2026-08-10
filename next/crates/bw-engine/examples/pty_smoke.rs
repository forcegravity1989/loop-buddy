//! `pty_smoke` — PTY 后端接缝的本机烟测(next 切片三B;切片三-1 修补输入
//! 与 resize 断言,评审 must-fix 顺手一并 #5)。自我标注:这是流程演示,
//! 不是单元测试(本仓核心纪律不写/不留单元测试,靠 headless 指挥器读回)。
//!
//! **不依赖 claude/网关**:用 [`bw_engine::pty_backend::active`] 选出的当前
//! 平台 PTY 后端起一个**普通 shell 探针脚本**(不是简单的 `echo` 一次性
//! 命令),证明「起进程 → 经 PTY 双向倒腾字节(含输入 + resize)→ 干净收
//! 尾」这条链路本体真的能跑——这是裁决 #7'(macOS 现实)要求的最小验收:
//! 不补 Unix 后端,这条链路在本机为零;补上以后,这个烟测就是它「真能工
//! 作」的证据,和 claude CLI 是否装了无关。
//!
//! **探针脚本**(评审的复现法):`read line; stty size; echo "got:$line"`
//! ——先阻塞等一行输入(证明「App → PTY」这条输入链路能工作,回显靠内核
//! PTY line discipline 自己回显写进主端的字节,不是应用层自己 echo)、再
//! 打印 `stty size`(证明 resize 请求真落到了 PTY 上,不是 no-op)。烟测
//! 按顺序先发 resize、再发输入行,`read` 解阻塞前 resize 必然已经生效,
//! 时序上不存在竞态。
//!
//! 跑法:`cd next && cargo run -p bw-engine --example pty_smoke`
//! 退出码 0、末行 `PTY_SMOKE_OK`、且读回字节里含输入回显与 resize 回报
//! = 通过。

use std::process::ExitCode;
use std::time::Duration;

use bw_engine::interactive_cli::{LaunchPlan, PtyInput};
use bw_engine::pty_backend::{self, PtyBackend};
use tokio::sync::mpsc;

// resize 目标尺寸:PTY 后端起 openpty 时给的初始默认值是 80x24(见
// `pty_backend::unix::UnixPtyBackend::run`),这里特意选一个明显不同的
// 值——120x40 若真出现在 `stty size` 的回报里,才证明 resize 真的生效,
// 不是碰巧撞上了默认值。
const RESIZE_COLS: u16 = 120;
const RESIZE_ROWS: u16 = 40;
const INPUT_LINE: &str = "bw-pty-smoke-input";

#[tokio::main]
async fn main() -> ExitCode {
    println!("== PTY 后端烟测(双向字节 + resize,普通 shell 探针,不依赖 claude)==");

    let workspace = std::env::temp_dir().join(format!("bw-pty-smoke-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!(
            "ASSERT FAILED: 无法建临时工作区 {}: {e}",
            workspace.display()
        );
        return ExitCode::FAILURE;
    }

    let plan = LaunchPlan {
        binary: "bash".to_string(),
        args: vec![
            "-c".to_string(),
            "read line; stty size; echo \"got:$line\"".to_string(),
        ],
        env: Default::default(),
        cwd: workspace.clone(),
        submit_prompt: false,
    };

    let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();

    let backend = pty_backend::active();
    // `run()` 得跑在独立 task 里——这次烟测要在它跑着的时候喂 resize/输
    // 入,不能像纯读单次输出那样直接 await 到底再看结果。
    let run_task =
        tokio::spawn(async move { backend.run("bash", &plan, bytes_tx, input_rx).await });

    // 先发 resize,再发输入行。探针脚本卡在 `read line` 上,直到收到输入
    // 行才会往下走到 `stty size`——只要 resize 先于输入行送达(mpsc 单通
    // 道 FIFO,后端 select! 循环按收到顺序处理),`stty size` 读到的必然
    // 是 resize 之后的尺寸,时序上没有竞态,这两个 sleep 只是给子进程一
    // 点起步余量,不是正确性的必要条件。
    tokio::time::sleep(Duration::from_millis(300)).await;
    if input_tx
        .send(PtyInput::Resize {
            cols: RESIZE_COLS,
            rows: RESIZE_ROWS,
        })
        .is_err()
    {
        eprintln!("ASSERT FAILED: resize 请求发送失败(input_rx 已提前关闭,后端可能提前退出了)");
        return ExitCode::FAILURE;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    if input_tx
        .send(PtyInput::Bytes(format!("{INPUT_LINE}\n").into_bytes()))
        .is_err()
    {
        eprintln!("ASSERT FAILED: 输入字节发送失败(input_rx 已提前关闭,后端可能提前退出了)");
        return ExitCode::FAILURE;
    }
    // **不要**在这里 drop(input_tx):后端 select! 循环把「输入端被丢」
    // (`input_rx.recv()` 返回 `None`)当成调用方主动喊停,会立刻 break
    // 出循环转入收尾——哪怕探针脚本还没跑到 `stty size`/`echo` 那两句也
    // 会被提前杀掉(本轮实测就是这样栽的:提前 drop 只读到了内核 line
    // discipline 的输入回显,`stty size`/`got:` 两行来不及产生)。正确
    // 姿势是按住发送端不放,让循环只经 `read_handle` 那支(子进程自己
    // 跑完探针脚本、自然退出、读到 EOF)收尾;`input_tx` 活到 `main` 函
    // 数结束自然 drop,那时 `run_task` 早已经 join 完成。

    // 15s 挂钟兜底——烟测本身不该卡死;真卡住了就是后端有 bug,不是本脚本
    // 该吞掉的东西。
    let run_result = tokio::time::timeout(Duration::from_secs(15), run_task).await;

    let _ = std::fs::remove_dir_all(&workspace);

    let output = match run_result {
        Ok(Ok(Ok(out))) => out,
        Ok(Ok(Err(e))) => {
            eprintln!("ASSERT FAILED: PTY 后端返回错误: {e}");
            return ExitCode::FAILURE;
        }
        Ok(Err(join_err)) => {
            eprintln!("ASSERT FAILED: 后端 task panic: {join_err}");
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
    // 回显断言(评审要求的「双向」证据的「入」这一半):内核 PTY line
    // discipline 把写进主端的输入字节原样回显到读端,这不是应用层自己
    // echo 出来的,是真的写进去了才有的字节。
    if !text.contains(INPUT_LINE) {
        eprintln!("ASSERT FAILED: 读回输出应含输入回显 {INPUT_LINE:?},实得: {text:?}");
        ok = false;
    }
    // 探针脚本收到输入后应该打印 "got:<输入行>"——证明 PTY 主端写进去的
    // 字节真的被从端的 shell 进程读到了(不只是内核 line discipline 层面
    // 的回显,连应用都吃到了)。
    let want_got = format!("got:{INPUT_LINE}");
    if !text.contains(&want_got) {
        eprintln!("ASSERT FAILED: 探针脚本应打印 {want_got:?},实得: {text:?}");
        ok = false;
    }
    // stty size 打印 "<rows> <cols>"(POSIX 惯例,Linux/macOS 一致);resize
    // 请求发的是 rows=40 cols=120——顺序对应上就证明 resize 真落到了 PTY,
    // 不是 no-op。
    let want_size = format!("{RESIZE_ROWS} {RESIZE_COLS}");
    if !text.contains(&want_size) {
        eprintln!(
            "ASSERT FAILED: 读回输出应含 stty size 回报 {want_size:?}(rows cols),实得: {text:?}"
        );
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
