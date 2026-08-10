//! `pty_smoke` — PTY 后端接缝的本机烟测(next 切片三B;切片三-1 修补输入
//! 与 resize 断言,评审 must-fix 顺手一并 #5;缺口清偿轮 2026-08-10 加 env-
//! strip 与自动提交两节)。自我标注:这是流程演示,不是单元测试(本仓核心
//! 纪律不写/不留单元测试,靠 headless 指挥器读回)。
//!
//! **不依赖 claude/网关**:三节都只用 [`bw_engine::pty_backend::active`] 选
//! 出的当前平台 PTY 后端起普通 shell 探针脚本,不碰真实 `claude` CLI 或
//! GLM 网关——确定性、每次门禁都能跑。
//!
//! 1. **双向字节 + resize**(裁决 #7'):`read line; stty size; echo
//!    "got:$line"` ——先阻塞等一行输入(证明「App → PTY」这条输入链路能工
//!    作,回显靠内核 PTY line discipline 自己回显写进主端的字节,不是应用
//!    层自己 echo)、再打印 `stty size`(证明 resize 请求真落到了 PTY 上)。
//! 2. **env-strip 真生效**(清偿 plan/23-opc-stitching-rebuild.md §10 第 3
//!    条):父进程(本指挥器自身)设一个毒环境变量,`LaunchPlan.env` 按
//!    `interactive_cli::build_startup_plan` 的真实剥离手法(全量拷贝
//!    `std::env::vars()` 再删掉目标键)构造——不是空 `HashMap`,这样才能
//!    复现「`CommandBuilder`/conpty-oxide `Command` 起步时已经整份继承父
//!    进程环境」这条真实故障路径。经 PTY 起
//!    `bash -c 'echo TOKEN=${ANTHROPIC_AUTH_TOKEN:-EMPTY}'`,读回必须是
//!    `TOKEN=EMPTY`。
//! 3. **Unix 自动提交**(清偿 plan/23-opc-stitching-rebuild.md §10 第 4
//!    条):`plan.submit_prompt = true` 时,后端应该在 TUI 起来后自己发一
//!    次 `\r`——探针脚本卡在 `read line` 上,指挥器全程不发任何
//!    `PtyInput::Bytes`,纯靠后端自己发的那个 `\r` 解阻塞。
//!
//! 跑法:`cd next && cargo run -p bw-engine --example pty_smoke`
//! 退出码 0、末行 `PTY_SMOKE_OK`、三节都印 `OK` = 通过。

use std::collections::HashMap;
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
    println!("== PTY 后端烟测(双向字节 + resize / env-strip / 自动提交,不依赖 claude)==");

    let mut ok = true;
    ok &= section_bidirectional_and_resize().await;
    println!();
    ok &= section_env_strip().await;
    println!();
    ok &= section_auto_submit().await;

    println!();
    if ok {
        println!("PTY_SMOKE_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("PTY_SMOKE_FAILED");
        ExitCode::FAILURE
    }
}

// ─── 1. 双向字节 + resize(裁决 #7' 的最小验收)──────────────────────────

async fn section_bidirectional_and_resize() -> bool {
    println!("== 1. 双向字节 + resize:普通 shell 探针 ==");

    let workspace = std::env::temp_dir().join(format!("bw-pty-smoke-bidir-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!(
            "ASSERT FAILED: 无法建临时工作区 {}: {e}",
            workspace.display()
        );
        return false;
    }

    let plan = LaunchPlan {
        binary: "bash".to_string(),
        args: vec![
            "-c".to_string(),
            "read line; stty size; echo \"got:$line\"".to_string(),
        ],
        // 缺口清偿轮之前这里是 `Default::default()`(空 `HashMap`)也能跑,
        // 是因为旧 bug 本身:`CommandBuilder` 起步整份继承父进程环境,空
        // `plan.env` 蒙混过关。`pty_backend.rs` 的 env_clear() 修复后
        // `plan.env` 真正是子进程环境的唯一来源——空 `HashMap` 现在会让子
        // 进程连 `PATH` 都没有,`bash` 都解析不出来。改成跟
        // `interactive_cli::build_startup_plan` 同款的「拷贝当前进程环境」,
        // 这是本节的正确姿势,不是走回头路。
        env: std::env::vars().collect(),
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
        return false;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    if input_tx
        .send(PtyInput::Bytes(format!("{INPUT_LINE}\n").into_bytes()))
        .is_err()
    {
        eprintln!("ASSERT FAILED: 输入字节发送失败(input_rx 已提前关闭,后端可能提前退出了)");
        return false;
    }
    // **不要**在这里 drop(input_tx):后端 select! 循环把「输入端被丢」
    // (`input_rx.recv()` 返回 `None`)当成调用方主动喊停,会立刻 break
    // 出循环转入收尾——哪怕探针脚本还没跑到 `stty size`/`echo` 那两句也
    // 会被提前杀掉(本轮实测就是这样栽的:提前 drop 只读到了内核 line
    // discipline 的输入回显,`stty size`/`got:` 两行来不及产生)。正确
    // 姿势是按住发送端不放,让循环只经 `read_handle` 那支(子进程自己
    // 跑完探针脚本、自然退出、读到 EOF)收尾;`input_tx` 活到函数结束自然
    // drop,那时 `run_task` 早已经 join 完成。

    // 15s 挂钟兜底——烟测本身不该卡死;真卡住了就是后端有 bug,不是本脚本
    // 该吞掉的东西。
    let run_result = tokio::time::timeout(Duration::from_secs(15), run_task).await;

    let _ = std::fs::remove_dir_all(&workspace);

    let output = match run_result {
        Ok(Ok(Ok(out))) => out,
        Ok(Ok(Err(e))) => {
            eprintln!("ASSERT FAILED: PTY 后端返回错误: {e}");
            return false;
        }
        Ok(Err(join_err)) => {
            eprintln!("ASSERT FAILED: 后端 task panic: {join_err}");
            return false;
        }
        Err(_) => {
            eprintln!("ASSERT FAILED: PTY 后端 15s 内未收尾(疑似卡死)");
            return false;
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
        println!("OK: 双向字节 + resize 全部通过");
    }
    ok
}

// ─── 2. env-strip 真生效(清偿 plan/23 §10 第 3 条)────────────────────

/// 父进程(本指挥器自身进程)设一个毒环境变量,`LaunchPlan.env` 按
/// `interactive_cli::build_startup_plan` 的真实剥离手法(全量拷贝
/// `std::env::vars()` 再删掉目标键)构造——**不是空 `HashMap`**,这样才能
/// 测出「`CommandBuilder`/conpty-oxide `Command` 起步时已经整份继承了父进
/// 程环境」这条真实故障路径,不是学术上的假想场景。经 PTY 起
/// `bash -c 'echo TOKEN=${ANTHROPIC_AUTH_TOKEN:-EMPTY}'`,读回必须是
/// `TOKEN=EMPTY`——真出现 `TOKEN=poison-test` 就证明剥离没有真的生效。
///
/// **突变自证**(本节新增时做过):把 `pty_backend.rs` unix 模块新加的
/// `cmd.env_clear();` 那一行注释掉,重跑这一节——读回变成
/// `TOKEN=poison-test`,断言按预期变红;改回来后恢复 `TOKEN=EMPTY`。结果记
/// 在任务报告,不是空话。
async fn section_env_strip() -> bool {
    println!("== 2. env-strip 真生效:毒变量经 PTY 起真实子进程,读回必须已剥离 ==");

    const POISON_VAR: &str = "ANTHROPIC_AUTH_TOKEN";
    const POISON_VALUE: &str = "poison-test";

    let workspace =
        std::env::temp_dir().join(format!("bw-pty-smoke-envstrip-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!(
            "ASSERT FAILED: 无法建临时工作区 {}: {e}",
            workspace.display()
        );
        return false;
    }

    // 还原用:设毒变量前先存旧值——指挥器可能本来就在带 token 的宿主会话
    // 里跑(这正是本条缺口要防的真实场景),不该销毁调用方原有的值。
    let saved = std::env::var(POISON_VAR).ok();
    std::env::set_var(POISON_VAR, POISON_VALUE);

    // 与 `interactive_cli::build_startup_plan` 同一手法:全量拷贝当前进程
    // 环境,再删掉目标键——这样 `plan.env` 里没有它,但**当前进程的真实
    // 环境仍然有**(`CommandBuilder::new`/`PtyCommand::new` 整份继承的正是
    // 这份真实环境,不是 `plan.env`),复现的正是 plan/23 §10 第 3 条记录
    // 的那条故障路径。
    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.remove(POISON_VAR);

    let plan = LaunchPlan {
        binary: "bash".to_string(),
        args: vec![
            "-c".to_string(),
            format!("echo TOKEN=${{{POISON_VAR}:-EMPTY}}"),
        ],
        env,
        cwd: workspace.clone(),
        submit_prompt: false,
    };

    let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (_input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();

    let backend = pty_backend::active();
    let run_result = tokio::time::timeout(
        Duration::from_secs(10),
        backend.run("bash", &plan, bytes_tx, input_rx),
    )
    .await;

    // 还原(不管上面成败都要还原,不能让这个探针污染指挥器进程后续的环境
    // 或后面几节)。
    match saved {
        Some(v) => std::env::set_var(POISON_VAR, v),
        None => std::env::remove_var(POISON_VAR),
    }
    let _ = std::fs::remove_dir_all(&workspace);

    let output = match run_result {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            eprintln!("ASSERT FAILED: env-strip 探针 PTY 后端返回错误: {e}");
            return false;
        }
        Err(_) => {
            eprintln!("ASSERT FAILED: env-strip 探针 10s 内未收尾(疑似卡死)");
            return false;
        }
    };
    println!("run() 返回: completed={}", output.completed);

    let mut collected = Vec::new();
    while let Ok(chunk) = bytes_rx.try_recv() {
        collected.extend_from_slice(&chunk);
    }
    let text = String::from_utf8_lossy(&collected);
    println!("读回字节: {text:?}");

    let mut ok = true;
    if text.contains(&format!("TOKEN={POISON_VALUE}")) {
        eprintln!(
            "ASSERT FAILED: 子进程读到了毒变量 {POISON_VAR}={POISON_VALUE}——env-strip 没有真\
             的生效"
        );
        ok = false;
    }
    if !text.contains("TOKEN=EMPTY") {
        eprintln!("ASSERT FAILED: 读回应含 TOKEN=EMPTY,实得: {text:?}");
        ok = false;
    }
    if ok {
        println!("OK: 毒变量 {POISON_VAR} 经 PTY 真实子进程读回已剥离(TOKEN=EMPTY)");
    }
    ok
}

// ─── 3. Unix 自动提交(清偿 plan/23 §10 第 4 条)───────────────────────

/// `plan.submit_prompt = true` 时,`pty_backend::unix::UnixPtyBackend::run`
/// 应该在 TUI 起来后等一小段时间自动发一次 `\r`——探针脚本卡在 `read line`
/// 上,**指挥器全程不发任何 `PtyInput::Bytes`**(只留着 `input_tx` 不
/// drop,防止提前收尾),纯靠后端自己发的那个 `\r` 把 `read line` 解阻塞。
///
/// 语义对齐 `pty_backend::windows::WindowsPtyBackend`(同款
/// `submit_delay`/`submitted` 结构,固定 2000ms 等 TUI 加载完再发)——这是
/// 等固定时长的启发式,不是真的侦测到了 TUI 就绪;真实 claude 场景下这段
/// 时间够不够,`agent_session --real` 才是真正的证据(该指挥器第 6 节),
/// 本节只证「后端确实会自己发 `\r`」这件事本身,不依赖 claude/网关。
///
/// **这条断言在修复前的状态**:补这个后端逻辑之前,Unix 后端里根本没有
/// `submit_delay` 分支,`read line` 永远等不到任何输入,本节必然 8s 超时
/// ASSERT FAILED——本节的存在本身就是「新功能有没有真的接上」的证据,不需
/// 要额外注释掉一行代码去自证。
async fn section_auto_submit() -> bool {
    println!("== 3. Unix 自动提交:submit_prompt=true 时后端应自己发 \\r ==");

    let workspace =
        std::env::temp_dir().join(format!("bw-pty-smoke-autosubmit-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!(
            "ASSERT FAILED: 无法建临时工作区 {}: {e}",
            workspace.display()
        );
        return false;
    }

    let plan = LaunchPlan {
        binary: "bash".to_string(),
        args: vec![
            "-c".to_string(),
            "read line; echo \"submitted:[$line]\"".to_string(),
        ],
        // 同第 1 节的注释:env_clear() 修复后空 `HashMap` 会让子进程连
        // `PATH` 都没有,`bash` 解析不出来——拷贝当前进程环境。
        env: std::env::vars().collect(),
        cwd: workspace.clone(),
        submit_prompt: true,
    };

    let (bytes_tx, mut bytes_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    // 刻意不发任何 `PtyInput::Bytes`——`_input_tx` 只是为了不让
    // `input_rx.recv()` 提前收到 `None` 把循环打断,持有到函数结束自然
    // drop,那时 `run()` 早已经收尾。
    let (_input_tx, input_rx) = mpsc::unbounded_channel::<PtyInput>();

    let backend = pty_backend::active();
    // 后端内部的 submit_delay 是 2000ms——给够余量(8s 挂钟兜底),不是本
    // 节该卡的地方;卡住就是自动提交真没发生,不是本脚本超时太短。
    let run_result = tokio::time::timeout(
        Duration::from_secs(8),
        backend.run("bash", &plan, bytes_tx, input_rx),
    )
    .await;

    let _ = std::fs::remove_dir_all(&workspace);

    let output = match run_result {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            eprintln!("ASSERT FAILED: 自动提交探针 PTY 后端返回错误: {e}");
            return false;
        }
        Err(_) => {
            eprintln!(
                "ASSERT FAILED: 自动提交探针 8s 内未收尾——没人发 PtyInput,说明后端没有自动\
                 发 \\r 把 `read line` 解阻塞(自动提交失效)"
            );
            return false;
        }
    };
    println!("run() 返回: completed={}", output.completed);

    let mut collected = Vec::new();
    while let Ok(chunk) = bytes_rx.try_recv() {
        collected.extend_from_slice(&chunk);
    }
    let text = String::from_utf8_lossy(&collected);
    println!("读回字节: {text:?}");

    if text.contains("submitted:[]") {
        println!("OK: 后端在 submit_prompt=true 时自动发了 \\r,`read line` 收到一个空行被解阻塞");
        true
    } else {
        eprintln!("ASSERT FAILED: 读回应含 \"submitted:[]\"(自动提交的空行),实得: {text:?}");
        false
    }
}
