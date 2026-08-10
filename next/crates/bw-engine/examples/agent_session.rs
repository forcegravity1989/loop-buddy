//! `agent_session` — 切片三收官验收指挥器(design-s3-agentcli.md §7)。
//!
//! 不开界面,直接调用 `bw-engine`/`bw-connector` 的公开 API,证明「agentcli
//! 层真的把 claude 接成一条能起、能续接、能取消的执行连接器」——这是切片三
//! 的行为验收件,不是单元测试(本仓 2026-07-17 起的核心纪律:不写/不留单
//! 元测试,行为正确性靠 E2E 指挥器)。
//!
//! **本档替代了 `interactive_cli.rs` 里随迁自 v1 的 `#[cfg(test)] mod
//! tests`**(约 290 行,design §5.5/主控裁决 #9):design 明文的四条等价断
//! 言——claude 装配参数逐字节一致 / 续接不带系统提示词 / 空会话号回落
//! `--continue` / 未核对的行如实拒绝——全部在下面「注册表机械证明」一节覆
//! 盖;测试模块已在本 commit 删除(先补指挥器,再删测试,顺序不可颠倒)。
//!
//! 六节,对应 design §7.1「四件要证的事」+ §7.2「类型断路」+ next 切片三-2
//! 修新增的第 5 节(I4):
//! 1. **注册表机械证明**(§5.3):claude/cursor 两行各跑一次计划构造,claude
//!    的装配参数与 v1 逐字节一致;cursor 如实拒绝——证明表驱动的分叉真的
//!    走通了,不是碰巧只有一条路。
//! 2. **类型断路**(§7.2 第 5 条):`ExecState` 穷举匹配,证明没有 `Done`
//!    档——编译期证明,不是运行时断言。
//! 3. **会话反查反证**(§7.1「续接真同路径」的反证部分):`discover_sessions`
//!    只认同路径,换一个路径找不到会话——只读文件系统。**`BW_CLAUDE_HOME`
//!    指向 scratch 目录,不碰用户真实 `~/.claude/projects/`这句话的作用域
//!    只到本节为止**(next 切片三-2 修 M5 的澄清):第 4/6 节没有覆盖这个
//!    环境变量,`AgentCliConnector::start`/`--real` 里调用的
//!    `discover_sessions` 会去读真实 `$HOME/.claude/projects/<encoded>`——
//!    对第 4 节那些一次性临时工作区路径,这个目录几乎必然不存在(只读,
//!    读不到就返回空,无副作用),但严格说不是「不碰」,是「碰了但读不
//!    到东西」,跟第 3 节刻意隔离的保证不是一回事,不要混着读。
//! 4. **mock 生命周期**(§7.2 第 1-4/6 条 + 两条故意失败路径):
//!    `AgentCliConnector` + `MockInteractiveExecutor`,证明「只起不等 /
//!    续接自动走 resume 计划 / 取消真触发收尾且不被后台事后完成覆盖 / 注
//!    入清单与落盘系统提示词一致 / 工作区不存在或分支不符如实拒绝」。
//! 5. **取消真杀进常绿**(I4,next 切片三-2 修新增):不依赖 claude/网关,
//!    用一个非注册表正式成员的 shell 探针脚本(`nohup` 出一个孙进程后自
//!    己继续跑着不退出)真实 spawn → `cancel()` → 按**进程组**
//!    (`pgrep -g <pgid>`)断言子孙进程全不在——这是评审给的真实复现法
//!    (`bash -c "nohup sleep 25 & echo started"`),对应
//!    `pty_backend.rs` unix 模块 `killpg_graceful` 的收尾。确定性、可复
//!    核,进 `AGENT_SESSION_OK` 断言面(仅 unix;非 unix 平台如实跳过)。
//! 6. **`--real`(可选,不进常绿门禁)**:真实 `claude` 会话尝试一次,读回
//!    `~/.claude/projects/<encoded>/<uuid>.jsonl` 存在且非空。网关抖动是已
//!    知常态,失败可安全重试;成败都打印,不影响其余各节的断言结果。
//!
//! 跑法:
//! ```text
//! cargo run -p bw-engine --example agent_session                      # 不带 --real,确定性
//! cargo run -p bw-engine --example agent_session -- --real            # 加真实 claude 会话
//! cargo run -p bw-engine --example agent_session -- \
//!     --inject skill=/path/to/skill.md --inject notes=/path/to/notes.md
//! ```
//! 退出码 0 且末行 `AGENT_SESSION_OK`(不带 `--real` 时确定性;带 `--real`
//! 时该节的成败会打印但不改变前四节的判定)。

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bw_connector::{
    CallCtx, ConnError, Connector, ExecSpec, ExecState, InjectBlock, ProjectBinding, RequestId,
    SessionEnd, StopReason,
};
use bw_core::ProjectId;
use bw_engine::agentcli::{assemble_system_prompt, discover_sessions, AgentCliConnector};
use bw_engine::interactive_cli::{
    build_resume_plan, build_startup_plan, InteractiveCliExecutor, InteractiveExecutor, LaunchPlan,
    MockInteractiveExecutor, PromptInjectionMode, PtyInput, SkillOutput, TuiAgentConfig, CLAUDE,
    CURSOR,
};
use bw_engine::{ExecError, RunCtx};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn mk_cx() -> CallCtx {
    CallCtx {
        req: RequestId::new(),
        timeout: None,
        cancel: CancellationToken::new(),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("参数错误:{e}");
            eprintln!(
                "用法:agent_session [--real] [--inject <标签>=<路径>]... [--dump-prompt <路径>]"
            );
            return ExitCode::FAILURE;
        }
    };

    let mut all_ok = true;

    all_ok &= section_registry_mechanical_proof();
    println!();
    all_ok &= section_type_closure();
    println!();
    all_ok &= section_discover_sessions_counter_evidence();
    println!();
    all_ok &= section_mock_lifecycle(&args).await;
    println!();
    all_ok &= section_cancel_real_kill().await;

    if args.real {
        println!();
        section_real().await; // 成败打印,不计入 all_ok(§7.3:不进常绿门禁)
    }

    println!();
    if all_ok {
        println!("AGENT_SESSION_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("AGENT_SESSION_FAILED");
        ExitCode::FAILURE
    }
}

struct Args {
    inject: Vec<(String, PathBuf)>,
    dump_prompt: Option<PathBuf>,
    real: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut argv = std::env::args().skip(1);
    let mut inject = Vec::new();
    let mut dump_prompt = None;
    let mut real = false;
    while let Some(a) = argv.next() {
        match a.as_str() {
            "--real" => real = true,
            "--inject" => {
                let kv = argv.next().ok_or("--inject 需要 <标签>=<路径> 参数")?;
                let (label, path) = kv
                    .split_once('=')
                    .ok_or("--inject 参数格式须是 <标签>=<路径>")?;
                inject.push((label.to_string(), PathBuf::from(path)));
            }
            "--dump-prompt" => {
                dump_prompt = Some(PathBuf::from(
                    argv.next().ok_or("--dump-prompt 需要一个路径参数")?,
                ));
            }
            other => return Err(format!("未知参数:{other}")),
        }
    }
    Ok(Args {
        inject,
        dump_prompt,
        real,
    })
}

// ─── 1. 注册表机械证明(design §5.3)──────────────────────────────────────

/// claude/cursor 两行各跑一次计划构造,打印装配出来的完整命令行。claude 那
/// 行的参数与 v1(`interactive_cli.rs` 被删测试模块里 `build_startup_plan_
/// claude_supported` 等用例断言过的形状)逐字节一致;cursor 那行如实拒绝
/// (「参数未核对,拒绝启动」),这条拒绝本身也是断言项。
fn section_registry_mechanical_proof() -> bool {
    println!("== 1. 注册表机械证明:claude 逐字节一致 / cursor 如实拒绝 ==");
    let mut ok = true;

    let tmp = std::env::temp_dir().join(format!("bw-agent-session-plan-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);

    // I3:env-strip 断言要先设两个哨兵值,不然「LaunchPlan.env 不含它们」
    // 是一句空验证(进程本来就没设,不删也通过)。设之前先存原值,断言完
    // 立刻还原——不打算长期改写这两个键。
    // **多线程 runtime 注意(M5)**:`std::env::set_var`/`remove_var` 改的
    // 是进程级全局状态,tokio 多线程 runtime 下若另一路任务同时在读环境
    // 变量,会看见这里改写过程中的中间态。本指挥器的 `main` 顺序 `.await`
    // 每一节,同一时刻没有别的任务在跑,不构成真实竞态——但这不是通用安
    // 全模式,换成并发场景(真的并行跑多个指挥器/测试)要害,不要照抄。
    let saved_token = std::env::var("ANTHROPIC_AUTH_TOKEN").ok();
    let saved_claudecode = std::env::var("CLAUDECODE").ok();
    std::env::set_var("ANTHROPIC_AUTH_TOKEN", "bw-agent-session-sentinel-token");
    std::env::set_var("CLAUDECODE", "1");

    // -- claude · 首启,不给 session_id(与 v1 基线逐字节一致;三-2 修
    //    之前 build_startup_plan 没有 session_id 形参,这个基线形状是「什
    //    么都不给」时的行为,不应该因为加了新形参就变形)--
    let position_prompt = "示例位置 prompt(这件活要做什么)";
    let system_prompt = "示例系统提示词(衔接层正文,占位)";
    match build_startup_plan(&CLAUDE, position_prompt, system_prompt, None, &tmp) {
        Ok(plan) => {
            println!(
                "claude 首启装配(无 session_id):{} {}",
                plan.binary,
                plan.args.join(" ")
            );
            let expected = vec![
                "--append-system-prompt".to_string(),
                system_prompt.to_string(),
                position_prompt.to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--disallowedTools".to_string(),
                "Bash(gh pr merge)".to_string(),
            ];
            ok &= assert_eq_vec(
                "claude 首启参数(无 session_id,与 v1 基线一致)",
                &plan.args,
                &expected,
            );
            // I3:被删测试模块里断言过的三项,指挥器补齐(design §5.5 的
            // 「等价断言」不能只顾 args,plan 的其余三个字段也要有断言,
            // 不能留空验证)。
            ok &= assert_eq_str("claude 首启 binary", &plan.binary, "claude");
            ok &= assert_env_stripped("claude 首启 env(嵌套会话变量剥离)", &plan.env);
            ok &= assert_eq_path("claude 首启 cwd", &plan.cwd, &tmp);
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: claude(verified=true)首启装配不应该失败,实得:{e}");
            ok = false;
        }
    }

    // 还原(M5):这两个键只是为了上面的 env-strip 断言临时借用。
    match saved_token {
        Some(v) => std::env::set_var("ANTHROPIC_AUTH_TOKEN", v),
        None => std::env::remove_var("ANTHROPIC_AUTH_TOKEN"),
    }
    match saved_claudecode {
        Some(v) => std::env::set_var("CLAUDECODE", v),
        None => std::env::remove_var("CLAUDECODE"),
    }

    // -- claude · 首启,给了 session_id(C1:会话号真接线)——这是本轮修
    //    复的核心行为:注册表行支持 session_id_flag 且调用方真给了值时,
    //    argv 里必须真的出现 --session-id <id>,位置紧跟系统提示词之后、
    //    位置 prompt 之前。
    let sample_session_id = "8dde12c8-6151-41c1-ba7a-9a7eddf00fa2";
    match build_startup_plan(
        &CLAUDE,
        position_prompt,
        system_prompt,
        Some(sample_session_id),
        &tmp,
    ) {
        Ok(plan) => {
            println!(
                "claude 首启装配(给了 session_id):{} {}",
                plan.binary,
                plan.args.join(" ")
            );
            let expected = vec![
                "--append-system-prompt".to_string(),
                system_prompt.to_string(),
                "--session-id".to_string(),
                sample_session_id.to_string(),
                position_prompt.to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--disallowedTools".to_string(),
                "Bash(gh pr merge)".to_string(),
            ];
            ok &= assert_eq_vec(
                "claude 首启参数(给了 session_id,C1 修复)",
                &plan.args,
                &expected,
            );
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 给了 session_id 的首启装配不应该失败,实得:{e}");
            ok = false;
        }
    }

    // -- claude · 首启,session_id_flag=Some 但调用方传空串(C1 的「缺一
    //    不推」分支:旗标存在 ≠ 调用方给了值,空串必须当没给处理)--
    match build_startup_plan(&CLAUDE, position_prompt, system_prompt, Some(""), &tmp) {
        Ok(plan) => {
            if plan.args.iter().any(|a| a == "--session-id") {
                eprintln!(
                    "ASSERT FAILED: session_id 传空串不应该推 --session-id,实得参数:{:?}",
                    plan.args
                );
                ok = false;
            } else {
                println!("OK: session_id 传空串 -> 不推 --session-id(旗标存在≠调用方给了值)");
            }
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: session_id 传空串不应该导致装配失败,实得:{e}");
            ok = false;
        }
    }

    // -- claude · 首启,system_prompt 为空(design §5.2「如实降级」)--
    match build_startup_plan(&CLAUDE, position_prompt, "", None, &tmp) {
        Ok(plan) => {
            let has_flag = plan.args.iter().any(|a| a == "--append-system-prompt");
            if has_flag {
                eprintln!(
                    "ASSERT FAILED: system_prompt 为空时不应该推 --append-system-prompt,\
                     实得参数:{:?}",
                    plan.args
                );
                ok = false;
            } else {
                println!("OK: system_prompt 为空 -> 不推 --append-system-prompt(如实降级)");
            }
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 空 system_prompt 不应该导致装配失败,实得:{e}");
            ok = false;
        }
    }

    // -- claude · 续接(有 session_id)--
    let session_id = "abc-123-session-id";
    match build_resume_plan(&CLAUDE, Some(session_id), &tmp) {
        Ok(plan) => {
            println!("claude 续接装配:{} {}", plan.binary, plan.args.join(" "));
            let expected = vec![
                "--resume".to_string(),
                session_id.to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--disallowedTools".to_string(),
                "Bash(gh pr merge)".to_string(),
            ];
            ok &= assert_eq_vec("claude 续接参数", &plan.args, &expected);
            if plan.args.iter().any(|a| a == "--append-system-prompt") {
                eprintln!("ASSERT FAILED: 续接计划不应该带 --append-system-prompt");
                ok = false;
            } else {
                println!("OK: 续接不带系统提示词旗标");
            }
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: claude 续接装配不应该失败,实得:{e}");
            ok = false;
        }
    }

    // -- claude · 续接,空会话号回落 --continue --
    match build_resume_plan(&CLAUDE, None, &tmp) {
        Ok(plan) => {
            println!(
                "claude 续接(空会话号)装配:{} {}",
                plan.binary,
                plan.args.join(" ")
            );
            let expected = vec![
                "--continue".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--disallowedTools".to_string(),
                "Bash(gh pr merge)".to_string(),
            ];
            ok &= assert_eq_vec("claude 续接(空会话号)参数", &plan.args, &expected);
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 空会话号应回落 --continue,不应该失败,实得:{e}");
            ok = false;
        }
    }

    // -- cursor · 首启/续接均如实拒绝 --
    match build_startup_plan(&CURSOR, position_prompt, system_prompt, None, &tmp) {
        Err(_) => println!("cursor: 参数未核对,拒绝启动(build_startup_plan 如实 Err)"),
        Ok(plan) => {
            eprintln!(
                "ASSERT FAILED: cursor(verified=false)不应该装配出计划,实得:{:?}",
                plan.args
            );
            ok = false;
        }
    }
    match build_resume_plan(&CURSOR, Some(session_id), &tmp) {
        Err(_) => println!("cursor: 参数未核对,拒绝启动(build_resume_plan 如实 Err)"),
        Ok(plan) => {
            eprintln!(
                "ASSERT FAILED: cursor(verified=false)续接不应该装配出计划,实得:{:?}",
                plan.args
            );
            ok = false;
        }
    }

    let _ = std::fs::remove_dir_all(&tmp);
    ok
}

fn assert_eq_vec(label: &str, got: &[String], expected: &[String]) -> bool {
    if got == expected {
        println!("OK: {label} 与预期逐字节一致({} 项)", expected.len());
        true
    } else {
        eprintln!("ASSERT FAILED: {label} 与预期不一致\n  预期: {expected:?}\n  实得: {got:?}");
        false
    }
}

/// I3:补齐被删测试模块里断言过、但指挥器第 1 节起初只顾了 `args` 的那几
/// 项——`plan.binary`。
fn assert_eq_str(label: &str, got: &str, expected: &str) -> bool {
    if got == expected {
        println!("OK: {label} = {expected:?}");
        true
    } else {
        eprintln!("ASSERT FAILED: {label} 期望 {expected:?},实得 {got:?}");
        false
    }
}

/// I3:`plan.cwd` 断言。
fn assert_eq_path(label: &str, got: &Path, expected: &Path) -> bool {
    if got == expected {
        println!("OK: {label} = {}", expected.display());
        true
    } else {
        eprintln!(
            "ASSERT FAILED: {label} 期望 {},实得 {}",
            expected.display(),
            got.display()
        );
        false
    }
}

/// I3:`plan.env` 剥离断言。**不是空验证**——调用方在调 `build_startup_plan`
/// 之前把这两个键设成了哨兵值(见调用处),这里命中断言才说明真的被
/// `build_startup_plan` 内部的 for 循环删掉了,不是「进程本来就没设」侥幸
/// 通过。
///
/// **如实标注这条断言证到哪一步、证不到哪一步**:这里只验证
/// `LaunchPlan.env`(一个 `HashMap`)不含这两个键——这是 `build_startup_plan`
/// 自己的契约。**「真实子进程也看不到这两个键」这一半,曾经证不到、已在缺
/// 口清偿轮(2026-08-10)补上独立证据**:那条链路
/// (`pty_backend::unix::UnixPtyBackend::run` 起步已改成先 `env_clear()` 再
/// 整个套用 `plan.env`,不再整份继承父进程环境)有了自己的确定性断言——见
/// `pty_smoke` 指挥器的 env-strip 探针节(经真实 PTY 起子进程读回,不依赖
/// claude/网关,每次门禁都跑),不在本节重复验证同一件事,这里仍然只管
/// `LaunchPlan.env` 这一层契约。
fn assert_env_stripped(label: &str, env: &std::collections::HashMap<String, String>) -> bool {
    let leaked: Vec<&str> = ["ANTHROPIC_AUTH_TOKEN", "CLAUDECODE"]
        .into_iter()
        .filter(|k| env.contains_key(*k))
        .collect();
    if leaked.is_empty() {
        println!(
            "OK: {label}(LaunchPlan.env 已剔除;真实子进程是否也看不到由 pty_smoke 的 \
             env-strip 探针节独立证明)"
        );
        true
    } else {
        eprintln!("ASSERT FAILED: {label} 仍残留于 LaunchPlan.env:{leaked:?}");
        false
    }
}

// ─── 2. 类型断路(design §7.2 第 5 条)────────────────────────────────────

/// 对 `ExecState`/`SessionEnd` 做一次穷举匹配,证明没有 `Done` 这一档,
/// `Finished` 的三档也没有一档能读成「已验收」。**这是编译期证明,不是运行
/// 时断言**——没有 `_` 通配分支,若契约新增 `Done` 档,这个函数编译不过,
/// 逼着这份指挥器同步更新,不会静默漏检。
fn section_type_closure() -> bool {
    println!("== 2. 类型断路:ExecState 穷举(编译期证明)==");
    let samples = [
        ExecState::Running,
        ExecState::Finished {
            ended: SessionEnd::ProcessExit { code: Some(0) },
            summary: "demo".into(),
        },
        ExecState::Finished {
            ended: SessionEnd::StoppedByBw {
                reason: StopReason::WallClock,
            },
            summary: "demo".into(),
        },
        ExecState::Finished {
            ended: SessionEnd::ContactLost,
            summary: "demo".into(),
        },
        ExecState::Canceled,
        ExecState::Orphaned,
        ExecState::Unknown,
    ];
    for s in &samples {
        let label = match s {
            ExecState::Running => "Running",
            ExecState::Finished {
                ended: SessionEnd::ProcessExit { .. },
                ..
            } => "Finished(ProcessExit)",
            ExecState::Finished {
                ended: SessionEnd::StoppedByBw { .. },
                ..
            } => "Finished(StoppedByBw)",
            ExecState::Finished {
                ended: SessionEnd::ContactLost,
                ..
            } => "Finished(ContactLost)",
            ExecState::Canceled => "Canceled",
            ExecState::Orphaned => "Orphaned",
            ExecState::Unknown => "Unknown",
            // 刻意没有 `_` 通配分支——见函数文档。
        };
        println!("{label}: 不是「已验收」的 Done 档");
    }
    println!(
        "穷举分支数: {}(无 `_` 通配,契约加 Done 档会导致这里编译失败,不会静默漏检)",
        samples.len()
    );
    true
}

// ─── 3. 会话反查反证(design §7.1「续接真同路径」的反证部分)────────────

/// `discover_sessions` 只读文件名与 mtime,只认同路径——伪造一份
/// `<BW_CLAUDE_HOME>/projects/<encoded-a>/<uuid>.jsonl`,断言反查得到它;换
/// 一个从没出现过的路径反查,断言为空。**本节验证全程**只碰
/// `BW_CLAUDE_HOME` 指向的 scratch 目录,不碰用户真实的
/// `~/.claude/projects/`——这句保证的作用域**只到本节为止**(next 切片
/// 三-2 修 M5):第 4/6 节没有设这个环境变量覆盖,里面调用
/// `discover_sessions`(经 `AgentCliConnector::start`,或 `--real` 直接调)
/// 会去读真实 `$HOME/.claude/projects/`,不在这条隔离保证范围内(见本文件
/// 顶部模块文档第 3 节条目的补充说明)。
///
/// **多线程 runtime 注意(M5)**:`std::env::set_var`/`remove_var` 改的是进
/// 程级全局状态;tokio 多线程 runtime 下若另一路任务同时在读环境变量,会
/// 看见这里改写过程中的中间态。本函数全程在 `main` 顺序 `.await` 的单一控
/// 制流里设置、使用、复原,同一时刻没有别的任务在跑,不构成真实竞态——但
/// 这不是通用安全模式,换成并发场景(真的并行跑多个指挥器/测试)会读到别
/// 的任务设的中间态,不要照抄到并发场景。
fn section_discover_sessions_counter_evidence() -> bool {
    println!("== 3. 反证:discover_sessions 只认同路径(不碰真实 ~/.claude/projects/)==");
    let mut ok = true;

    let fake_home = std::env::temp_dir().join(format!(
        "bw-agent-session-claude-home-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&fake_home);
    std::fs::create_dir_all(&fake_home).expect("scratch BW_CLAUDE_HOME 应可创建");
    std::env::set_var("BW_CLAUDE_HOME", &fake_home);

    let workspace_a = PathBuf::from("/tmp/bw-agent-session-demo-workspace-a");
    let workspace_b = PathBuf::from("/tmp/bw-agent-session-demo-workspace-b-never-registered");
    let fake_session_id = "11111111-1111-1111-1111-111111111111";

    let encoded_a: String = workspace_a
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    let session_dir_a = fake_home.join("projects").join(&encoded_a);
    std::fs::create_dir_all(&session_dir_a).expect("伪造 session 目录应可创建");
    std::fs::write(
        session_dir_a.join(format!("{fake_session_id}.jsonl")),
        "{\"fake\":true,\"note\":\"agent_session 指挥器伪造的验证数据,不解析内容\"}\n",
    )
    .expect("伪造 jsonl 应可写入");

    let found_a = discover_sessions(&workspace_a);
    let found_b = discover_sessions(&workspace_b);
    println!("workspace_a 反查结果: {found_a:?}");
    println!("workspace_b(未注册路径)反查结果: {found_b:?}");

    if found_a.iter().any(|(id, _)| id == fake_session_id) {
        println!("OK: 同路径反查命中伪造会话");
    } else {
        eprintln!("ASSERT FAILED: workspace_a 应反查到 {fake_session_id},实得 {found_a:?}");
        ok = false;
    }
    if found_b.is_empty() {
        println!("OK: 换一个路径反查为空——续接不会串到别的工作区");
    } else {
        eprintln!("ASSERT FAILED: workspace_b 是从没注册过的路径,应反查为空,实得 {found_b:?}");
        ok = false;
    }

    let _ = std::fs::remove_dir_all(&fake_home);
    std::env::remove_var("BW_CLAUDE_HOME");
    ok
}

// ─── 4. mock 生命周期(design §7.2 第 1-4/6 条 + 两条故意失败路径)──────

/// 起一个真实(临时)git 工作区,在给定分支上有一次提交——`AgentCliConnector
/// ::start` 的 ①②两步校验需要真实的 `.git` 与真实的当前分支。
fn mk_git_workspace(branch: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "bw-agent-session-ws-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("临时工作区应可创建");
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .status()
            .expect("git 应可执行(本机需要装 git)");
        assert!(status.success(), "git {args:?} 应成功");
    };
    run(&["init", "-q", "-b", branch]);
    run(&["config", "user.email", "agent-session@bw.local"]);
    run(&["config", "user.name", "agent_session 指挥器"]);
    std::fs::write(dir.join("README.md"), "agent_session 指挥器临时工作区\n")
        .expect("README 应可写入");
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    dir
}

/// 延迟包装:先 `sleep`,再委托给内层 mock。给 mock 生命周期一个可控的
/// 「还没结束」观察窗口(裸 `MockInteractiveExecutor` 几乎瞬间完成,直接用
/// 它测不出 `poll` 在结束前报 `Running` 这件事)。
struct DelayedMock {
    inner: MockInteractiveExecutor,
    delay: Duration,
}

#[async_trait]
impl InteractiveExecutor for DelayedMock {
    async fn run_skill(&self, plan: &LaunchPlan, ctx: &RunCtx) -> Result<SkillOutput, ExecError> {
        tokio::time::sleep(self.delay).await;
        self.inner.run_skill(plan, ctx).await
    }

    async fn run_skill_resume(
        &self,
        plan: &LaunchPlan,
        ctx: &RunCtx,
    ) -> Result<SkillOutput, ExecError> {
        tokio::time::sleep(self.delay).await;
        self.inner.run_skill_resume(plan, ctx).await
    }

    async fn run_skill_pty(
        &self,
        plan: &LaunchPlan,
        ctx: &RunCtx,
        bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
        input_rx: mpsc::UnboundedReceiver<PtyInput>,
    ) -> Result<SkillOutput, ExecError> {
        tokio::time::sleep(self.delay).await;
        self.inner
            .run_skill_pty(plan, ctx, bytes_tx, input_rx)
            .await
    }
}

/// 读入 `--inject` 给的真实文件;没给时自建一份自我标注的演示文件(仍然是
/// 「从文件读」,不是内嵌字符串直接当 InjectBlock——design §6.2「指挥器演
/// 示时,注入块从命令行给的真实文件读」)。
fn load_inject_blocks(args: &Args) -> (Vec<InjectBlock>, Option<PathBuf>) {
    if !args.inject.is_empty() {
        let mut blocks = Vec::new();
        for (label, path) in &args.inject {
            match std::fs::read_to_string(path) {
                Ok(body) => blocks.push(InjectBlock {
                    label: label.clone(),
                    body,
                }),
                Err(e) => {
                    eprintln!("警告:--inject {label}={} 读取失败,跳过:{e}", path.display());
                }
            }
        }
        (blocks, None)
    } else {
        let dir = std::env::temp_dir().join(format!(
            "bw-agent-session-demo-inject-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("demo-skill.md");
        let _ = std::fs::write(
            &path,
            "【指挥器演示】这是 agent_session 自建的演示注入块正文,不是内置技能库\
             ——没有 --inject 参数时的默认样例,证明装配管线真的从文件读,不是内嵌字符串。\n",
        );
        (
            vec![InjectBlock {
                label: "demo".to_string(),
                body: std::fs::read_to_string(&path).unwrap_or_default(),
            }],
            Some(path),
        )
    }
}

async fn section_mock_lifecycle(args: &Args) -> bool {
    println!("== 4. mock 生命周期:只起不等 / 续接 / 取消(真触发收尾)/ 注入读回 / 两条故意失败 ==");
    let mut ok = true;

    // -- 4a. 故意失败:工作区不存在 --
    {
        let binding = ProjectBinding {
            project: ProjectId::new(),
            host: String::new(),
            path: "agent-session-demo".into(),
        };
        let conn =
            AgentCliConnector::new(binding, &CLAUDE, Arc::new(MockInteractiveExecutor::new()));
        let execute = conn.as_execute().expect("AgentCliConnector 应支持 Execute");
        let cx = mk_cx();
        let spec = ExecSpec {
            workspace: PathBuf::from("/definitely/does/not/exist/bw-agent-session"),
            branch: "main".into(),
            inject: vec![],
            budget_usd: None,
        };
        match execute.start(&cx, spec).await {
            Err(f) => match f.err {
                ConnError::NotConnected(msg) => {
                    println!("OK: 工作区不存在 -> NotConnected({msg})")
                }
                other => {
                    eprintln!("ASSERT FAILED: 工作区不存在应落 NotConnected,实得 {other:?}");
                    ok = false;
                }
            },
            Ok(_) => {
                eprintln!("ASSERT FAILED: 工作区不存在居然 start 成功(假绿)");
                ok = false;
            }
        }
    }

    // -- 4b. 故意失败:当前分支与要求不符 --
    let ws = mk_git_workspace("main");
    {
        let binding = ProjectBinding {
            project: ProjectId::new(),
            host: String::new(),
            path: "agent-session-demo".into(),
        };
        let conn =
            AgentCliConnector::new(binding, &CLAUDE, Arc::new(MockInteractiveExecutor::new()));
        let execute = conn.as_execute().unwrap();
        let cx = mk_cx();
        let spec = ExecSpec {
            workspace: ws.clone(),
            branch: "not-the-checked-out-branch".into(),
            inject: vec![],
            budget_usd: None,
        };
        match execute.start(&cx, spec).await {
            Err(f) => match f.err {
                ConnError::UpstreamRejected { message } => {
                    println!("OK: 分支不符 -> UpstreamRejected({message})")
                }
                other => {
                    eprintln!("ASSERT FAILED: 分支不符应落 UpstreamRejected,实得 {other:?}");
                    ok = false;
                }
            },
            Ok(_) => {
                eprintln!("ASSERT FAILED: 分支不符居然 start 成功(在错分支上放 agent 干活,假绿)");
                ok = false;
            }
        }
    }

    // -- 4c. 正常首启:只起不等 + 注入装配读回 + 续接 --
    let (inject_blocks, self_built_inject_path) = load_inject_blocks(args);
    // I1:`assemble_system_prompt` 现在多两个形参(workspace/branch,拼衔接
    // 层正文要用)——传下面 spec 里将要用的同一对值(ws / "main"),这样
    // `expected_prompt` 才是真的跟 `AgentCliConnector::start` 内部装配出来
    // 的同一份内容(读回比对的前提是「拿同样的输入算」)。
    let (expected_prompt, expected_records) = assemble_system_prompt(&ws, "main", &inject_blocks);
    let dump_path = args.dump_prompt.clone().unwrap_or_else(|| {
        std::env::temp_dir().join(format!(
            "bw-agent-session-prompt-{}.txt",
            std::process::id()
        ))
    });
    if let Err(e) = std::fs::write(&dump_path, &expected_prompt) {
        eprintln!(
            "ASSERT FAILED: 落盘系统提示词失败({}):{e}",
            dump_path.display()
        );
        ok = false;
    } else {
        println!("装配好的系统提示词已落盘: {}", dump_path.display());
    }

    // F2(评审 task-s3b-fixreview.md,next 切片三-2 修二新增):I1 保的是
    // 「衔接层正文(通用铁律)真的进了装配出的系统提示词」——上面只把结
    // 果落盘等人去看 dump 文件,从没断言过正文本身。三句标志句摘自
    // `interactive_cli.rs`「## 通用铁律(不可违反)」段的真实原句(无条
    // 件 `push_str`,不读任何 ctx 字段),不是新造的句子。
    let bridge_markers = ["Done 永不自动", "绝不伪造观测", "合并永远是人手动作"];
    let missing_markers: Vec<&str> = bridge_markers
        .iter()
        .copied()
        .filter(|m| !expected_prompt.contains(m))
        .collect();
    if missing_markers.is_empty() {
        println!("OK: 装配出的系统提示词含衔接层铁律正文的标志句(I1)");
    } else {
        eprintln!("ASSERT FAILED: 系统提示词缺铁律正文标志句:{missing_markers:?}");
        ok = false;
    }
    // 设计稿 §6.1 明文顺序的另一半:衔接层正文在前、inject 块在后——正文
    // 必须出现在第一个注入块标题(`## <label>`)之前。`load_inject_blocks`
    // 在没传 `--inject` 时也会自建一条演示块,`inject_blocks` 实际不会为
    // 空,这条顺序断言常态下真的在验证。
    if let Some(first_label) = inject_blocks.first().map(|b| format!("## {}", b.label)) {
        match (
            expected_prompt.find(bridge_markers[0]),
            expected_prompt.find(&first_label),
        ) {
            (Some(bridge_pos), Some(label_pos)) if bridge_pos < label_pos => {
                println!("OK: 铁律正文出现在第一个注入块标题之前(§6.1 顺序)")
            }
            (bridge_pos, label_pos) => {
                eprintln!(
                    "ASSERT FAILED: 铁律正文应在第一个注入块标题之前,实得 bridge_pos={bridge_pos:?} label_pos={label_pos:?}"
                );
                ok = false;
            }
        }
    }

    if let Some(p) = &self_built_inject_path {
        println!("(未传 --inject,自建演示注入文件: {})", p.display());
    }

    let binding = ProjectBinding {
        project: ProjectId::new(),
        host: String::new(),
        path: "agent-session-demo".into(),
    };
    let conn = AgentCliConnector::new(binding, &CLAUDE, Arc::new(MockInteractiveExecutor::new()));
    let execute = conn.as_execute().unwrap();

    let cx = mk_cx();
    let req1 = cx.req;
    let spec = ExecSpec {
        workspace: ws.clone(),
        branch: "main".into(),
        inject: inject_blocks,
        budget_usd: Some(9.99),
    };
    let started = std::time::Instant::now();
    let ticket1 = match execute.start(&cx, spec).await {
        Ok(ok_val) => ok_val.value,
        Err(f) => {
            eprintln!("ASSERT FAILED: 正常首启不应该失败:{}", f.err);
            return false;
        }
    };
    let elapsed = started.elapsed();
    println!(
        "start() 耗时: {elapsed:?}(证「只起不等」)· upstream_session={:?}",
        ticket1.upstream_session
    );
    if elapsed >= Duration::from_millis(500) {
        eprintln!(
            "ASSERT FAILED: start() 耗时 {elapsed:?} >= 500ms,不像是「立刻返回不等会话结束」"
        );
        ok = false;
    } else {
        println!("OK: start() 立刻返回");
    }

    // F1(评审 task-s3b-fixreview.md,next 切片三-2 修二新增):C1 保的判
    // 据是「能被指派会话号的行,upstream_session 真的编了号、不留空」——
    // 上面一直只打印读回,从没断言过。这是全新工作区首启(没有历史可续
    // 接),CLAUDE 的 `session_id_flag` 是 `Some(...)`(见
    // `interactive_cli.rs` CLAUDE 常量),连接器必须走 C1「能指派就编号」
    // 分支,`upstream_session` 不该是 `None`。对称的「不能指派就留空」分
    // 支由第 5 节的探针行(`session_id_flag: None`)断言。
    if ticket1.upstream_session.is_some() {
        println!("OK: claude(session_id_flag=Some)首启真编了 upstream_session,不留空(C1)");
    } else {
        eprintln!(
            "ASSERT FAILED: claude 首启应该编号 upstream_session,实得 None(C1 连接器语义退化)"
        );
        ok = false;
    }

    match conn.session_row(req1) {
        Some(row) => {
            println!(
                "首启会话行:injected={:?} budget_enforced={}",
                row.injected, row.budget_enforced
            );
            if row.injected == expected_records {
                println!("OK: SessionRow.injected 与落盘系统提示词的装配结果一致(标签/字节数/指纹三项对得上)");
            } else {
                eprintln!(
                    "ASSERT FAILED: injected 清单不一致\n  期望: {expected_records:?}\n  实得: {:?}",
                    row.injected
                );
                ok = false;
            }
            // M3(next 切片三-2 修):这条不是真断言,只是读回打印——
            // `budget_enforced` 在 `connector.rs` 里是硬编码
            // `let budget_enforced = false;`(design §8/主控裁决 #3:claude
            // 交互式接不上按次预算封顶),不是从 `budget_usd` 或任何输入推
            // 出来的值。不管上面 `spec.budget_usd` 传了什么,这一行恒为
            // `false`,`if row.budget_enforced { fail }` 那种写法永远进不
            // 去分支——挂着 ASSERT FAILED 的措辞会让读的人以为这里验证了
            // 什么行为,其实只是在验证一个常量字面量,删掉误导性的分支,
            // 老实打印读回值。
            println!(
                "读回(非断言,budget_enforced 是 connector.rs 里的硬编码 false,\
                 不随 budget_usd 变化): budget_enforced={}",
                row.budget_enforced
            );
        }
        None => {
            eprintln!("ASSERT FAILED: 首启后应该能读到 SessionRow,实得 None");
            ok = false;
        }
    }

    // mock 几乎瞬间完成,给后台任务一点调度机会。
    tokio::time::sleep(Duration::from_millis(80)).await;
    let cx_poll = mk_cx();
    match execute.poll(&cx_poll, &ticket1).await {
        Ok(ok_val) => match ok_val.value {
            ExecState::Finished { ended, summary } => {
                println!("OK: poll 报 Finished({ended:?})——mock 已完成,summary={summary:?}");
                // I3:补齐「mock 分支 summary 含【mock】」断言——本仓核心
                // 纪律第 2 条(「mock 必须自我标注」)的唯一守卫,之前这里
                // 用 `{ ended, .. }` 把 `summary` 直接扔了,断言面是空的。
                if summary.contains("【mock】") {
                    println!("OK: summary 含【mock】自我标注(核心纪律第 2 条)");
                } else {
                    eprintln!(
                        "ASSERT FAILED: mock 路径的 summary 必须自我标注【mock】,实得:{summary:?}"
                    );
                    ok = false;
                }
            }
            other => {
                eprintln!("ASSERT FAILED: mock 应已完成 Finished,实得 {other:?}");
                ok = false;
            }
        },
        Err(f) => {
            eprintln!("ASSERT FAILED: poll 不应该失败:{}", f.err);
            ok = false;
        }
    }

    // cancel 已结束的票据 -> 幂等 Ok。
    let cx_cancel = mk_cx();
    if let Err(f) = execute.cancel(&cx_cancel, &ticket1).await {
        eprintln!(
            "ASSERT FAILED: cancel 已结束的票据应该幂等成功,实得:{}",
            f.err
        );
        ok = false;
    } else {
        println!("OK: cancel 已结束的票据幂等(= Ok)");
    }

    // -- 续接:同一工作区第二次 start,自动走续接计划 --
    let cx2 = mk_cx();
    let req2 = cx2.req;
    let spec2 = ExecSpec {
        workspace: ws.clone(),
        branch: "main".into(),
        inject: vec![InjectBlock {
            label: "should-not-be-injected".into(),
            body: "续接不该看到这块".into(),
        }],
        budget_usd: None,
    };
    match execute.start(&cx2, spec2).await {
        Ok(ok_val) => {
            let ticket2 = ok_val.value;
            if ticket2.upstream_session == ticket1.upstream_session {
                println!(
                    "OK: 续接沿用同一个 upstream_session={:?}",
                    ticket2.upstream_session
                );
            } else {
                eprintln!(
                    "ASSERT FAILED: 续接应沿用同一个 upstream_session,首启={:?} 续接={:?}",
                    ticket1.upstream_session, ticket2.upstream_session
                );
                ok = false;
            }
            match conn.session_row(req2) {
                Some(row) if row.injected.is_empty() => {
                    println!("OK: 续接会话行 injected 为空(复利块首启时已进,续接不重注入,§6.3)")
                }
                Some(row) => {
                    eprintln!(
                        "ASSERT FAILED: 续接不应重新注入,实得 injected={:?}",
                        row.injected
                    );
                    ok = false;
                }
                None => {
                    eprintln!("ASSERT FAILED: 续接后应该能读到 SessionRow,实得 None");
                    ok = false;
                }
            }
        }
        Err(f) => {
            eprintln!("ASSERT FAILED: 续接 start 不应该失败:{}", f.err);
            ok = false;
        }
    }

    // -- 4d. 取消竞态:还在跑的会话被 cancel,后台事后完成不得覆盖 Canceled --
    let ws2 = mk_git_workspace("main");
    let binding2 = ProjectBinding {
        project: ProjectId::new(),
        host: String::new(),
        path: "agent-session-demo-cancel".into(),
    };
    let delayed = DelayedMock {
        inner: MockInteractiveExecutor::new(),
        delay: Duration::from_millis(300),
    };
    let conn2 = AgentCliConnector::new(binding2, &CLAUDE, Arc::new(delayed));
    let execute2 = conn2.as_execute().unwrap();

    let cx3 = mk_cx();
    let req3 = cx3.req;
    let spec3 = ExecSpec {
        // M5(指挥器卫生):clone,不是移动——`ws2` 这个 PathBuf 留着给函
        // 数末尾清理临时目录用,之前直接把它 move 进 `spec3` 之后就再也拿
        // 不到这个路径清理了(残留在 /tmp 下)。
        workspace: ws2.clone(),
        branch: "main".into(),
        inject: vec![],
        budget_usd: None,
    };
    let ticket3 = match execute2.start(&cx3, spec3).await {
        Ok(ok_val) => ok_val.value,
        Err(f) => {
            eprintln!("ASSERT FAILED: 取消竞态场景的 start 不应该失败:{}", f.err);
            return false;
        }
    };

    let cx4 = mk_cx();
    match execute2.poll(&cx4, &ticket3).await {
        Ok(ok_val) => match ok_val.value {
            ExecState::Running => println!("OK: 延迟窗口内 poll -> Running(还没到 300ms 延迟)"),
            other => {
                eprintln!("ASSERT FAILED: 延迟窗口内应仍是 Running,实得 {other:?}");
                ok = false;
            }
        },
        Err(f) => {
            eprintln!("ASSERT FAILED: poll 不应该失败:{}", f.err);
            ok = false;
        }
    }

    let cx5 = mk_cx();
    if let Err(f) = execute2.cancel(&cx5, &ticket3).await {
        eprintln!("ASSERT FAILED: cancel 运行中的票据不应该失败:{}", f.err);
        ok = false;
    }
    match conn2.session_row(req3) {
        Some(row) if row.state == ExecState::Canceled => {
            println!("OK: cancel 后立刻读回 -> Canceled")
        }
        Some(row) => {
            eprintln!(
                "ASSERT FAILED: cancel 后应立刻是 Canceled,实得 {:?}",
                row.state
            );
            ok = false;
        }
        None => {
            eprintln!("ASSERT FAILED: cancel 后应该还能读到 SessionRow,实得 None");
            ok = false;
        }
    }

    // 等过延迟窗口,让后台任务真正跑完,确认没有覆盖 Canceled(并发纪律)。
    tokio::time::sleep(Duration::from_millis(500)).await;
    match conn2.session_row(req3) {
        Some(row) if row.state == ExecState::Canceled => {
            println!("OK: 后台任务事后跑完,未覆盖 Canceled(cancel 先落定的状态优先)")
        }
        Some(row) => {
            eprintln!(
                "ASSERT FAILED: 后台完成不应该覆盖 Canceled,实得 {:?}(并发纪律失守)",
                row.state
            );
            ok = false;
        }
        None => {
            eprintln!("ASSERT FAILED: 应该还能读到 SessionRow,实得 None");
            ok = false;
        }
    }

    let _ = std::fs::remove_dir_all(&ws);
    // M5(指挥器卫生):`ws2`(4d 取消竞态那个临时工作区)之前从没清理
    // 过,残留在 /tmp 下——补上,跟 `ws` 一样善后。
    let _ = std::fs::remove_dir_all(&ws2);

    ok
}

// ─── 5. 取消真杀进常绿(I4,next 切片三-2 修新增)────────────────────────

/// I4:「取消真杀」进常绿门禁。第 4 节的取消竞态测的是**账本**(`SessionRow.
/// state` 变成 `Canceled` 且不被后台事后覆盖)——mock 从不真 spawn 进程,测
/// 不出 `pty_backend.rs` unix 模块那段 `killpg_graceful` 真的杀掉了子孙进
/// 程。本节补上这条,用真实 `InteractiveCliExecutor`(经真实 PTY 后端),但
/// 不依赖 claude/网关:造一个**非注册表正式成员**的「探针行」,`launch_cmd`
/// 指向一个临时 shell 脚本——脚本 `nohup` 出一个孙进程(`sleep 300`,脱离
/// 父进程独立存活)后自己也 `sleep` 住不退出(保证 `cancel()` 之前脚本进程
/// 一直是 `Running`,不会自然 EOF 把「是 cancel 触发的」这条断言蒙混过
/// 去——`pty_backend.rs` 的收尾代码在自然 EOF 与 cancel 两条路径之后都会跑
/// 同一段 `killpg_graceful`,脚本秒退就测不出取消本身有没有起作用)。
///
/// 起会话后记下探针脚本进程的 pid,查它的 pgid(`portable-pty` 的
/// `CommandBuilder::spawn_command` 对每个子进程调用 `setsid()`,顶层子进程
/// 的 pid 就是整个组的 pgid);`cancel()` 之后按**进程组**
/// (`pgrep -g <pgid>`)断言组内一个成员都不剩——这正是评审给的复现法
/// (`bash -c "nohup sleep 25 & echo started"`)的确定性、可复核版本。
///
/// 仅 `unix`——`nohup`/`pgrep`/`ps -o pgid=`/POSIX shell 脚本都是 unix 概
/// 念,本仓 CI(`ubuntu-latest`)与本机部署机(macOS)都是 unix,Windows 侧
/// 的 killpg 等价机制不在本节验证范围(如实登记在
/// `plan/23-opc-stitching-rebuild.md` 已有的 Windows 相关缺口条目里,不是
/// 本节新缺口)。
#[cfg(unix)]
async fn section_cancel_real_kill() -> bool {
    println!("== 5. 取消真杀进常绿:非 claude 行 · nohup 孙进程按进程组连坐 ==");
    let mut ok = true;

    let script_dir = std::env::temp_dir().join(format!(
        "bw-agent-session-cancel-probe-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    if let Err(e) = std::fs::create_dir_all(&script_dir) {
        eprintln!("ASSERT FAILED: 探针脚本目录创建失败:{e}");
        return false;
    }
    let script_path = script_dir.join("probe.sh");
    // 探针脚本:nohup 出一个孙进程(sleep 300,脱离父进程独立存活)后,自
    // 己也 sleep 住不退出——理由见函数文档「保证 cancel 之前脚本进程一直
    // 是 Running」。
    let script = "#!/bin/sh\nnohup sleep 300 >/dev/null 2>&1 &\necho probe-started\nsleep 300\n";
    if let Err(e) = std::fs::write(&script_path, script) {
        eprintln!("ASSERT FAILED: 探针脚本写入失败:{e}");
        let _ = std::fs::remove_dir_all(&script_dir);
        return false;
    }
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(&script_path) {
            Ok(meta) => {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&script_path, perms);
            }
            Err(e) => {
                eprintln!("ASSERT FAILED: 探针脚本 chmod +x 失败:{e}");
                let _ = std::fs::remove_dir_all(&script_dir);
                return false;
            }
        }
    }
    let script_path_str = script_path.to_string_lossy().into_owned();

    // 非 claude 的「注册表行」——`TuiAgentConfig.launch_cmd` 是
    // `&'static str`,常规注册表行在编译期写死;这里运行期现造一个探针路
    // 径,只能 `Box::leak`(指挥器这种短生命周期程序里唯一简单的做法,不
    // 是产品代码模式)。
    let launch_cmd: &'static str = Box::leak(script_path_str.clone().into_boxed_str());
    let probe_row: &'static TuiAgentConfig = Box::leak(Box::new(TuiAgentConfig {
        slug: "shell-probe(非注册表正式成员,仅本节验证用)",
        detect_cmd: "true",
        launch_cmd,
        prompt_injection_mode: PromptInjectionMode::PositionalArgv,
        yolo_flag: None,
        resume_flag: "--continue",
        resume_id_flag: "--resume",
        system_prompt_flag: None,
        session_id_flag: None,
        deny_tools: &[],
        verified: true,
    }));

    let ws = mk_git_workspace("main");
    let binding = ProjectBinding {
        project: ProjectId::new(),
        host: String::new(),
        path: "agent-session-cancel-real-kill".into(),
    };
    // 真实 InteractiveCliExecutor(经 pty_backend 真 spawn)——不是 mock,
    // mock 从不真起进程,测不出 killpg。
    let conn = AgentCliConnector::new(binding, probe_row, Arc::new(InteractiveCliExecutor::new()));
    let execute = conn.as_execute().unwrap();
    let cx = mk_cx();
    let req = cx.req;
    let spec = ExecSpec {
        workspace: ws.clone(),
        branch: "main".into(),
        inject: vec![],
        budget_usd: None,
    };
    let ticket = match execute.start(&cx, spec).await {
        Ok(ok_val) => ok_val.value,
        Err(f) => {
            eprintln!("ASSERT FAILED: 探针脚本 start 不应该失败:{}", f.err);
            let _ = std::fs::remove_dir_all(&ws);
            let _ = std::fs::remove_dir_all(&script_dir);
            return false;
        }
    };

    // F1(评审 task-s3b-fixreview.md,next 切片三-2 修二新增):C1「不能指
    // 派就留空,绝不填一个猜的号」的分支,本节探针行(`session_id_flag:
    // None`)天然在跑这条分支——之前只起了会话,从没读回断言过「真的留
    // 空」这句话。对称的「能指派就编号」分支由第 4 节 claude 行断言。
    if ticket.upstream_session.is_none() {
        println!("OK: session_id_flag=None 的探针行,票据 upstream_session 如实留空(C1)");
    } else {
        eprintln!(
            "ASSERT FAILED: session_id_flag=None 不该编号,票据 upstream_session 实得 {:?}",
            ticket.upstream_session
        );
        ok = false;
    }
    match conn.session_row(req) {
        Some(row) if row.upstream_session.is_empty() => {
            println!("OK: SessionRow.upstream_session 同样如实留空,不是编的号(C1)")
        }
        Some(row) => {
            eprintln!(
                "ASSERT FAILED: SessionRow.upstream_session 应留空,实得 {:?}",
                row.upstream_session
            );
            ok = false;
        }
        None => {
            eprintln!("ASSERT FAILED: 起会话后应该能读到 SessionRow,实得 None");
            ok = false;
        }
    }

    // 轮询等脚本真的跑起来(exec 探针脚本 + nohup 出孙进程)——固定
    // `sleep` 在系统繁忙时会不够长导致假失败(本轮修复时实测复现过一
    // 次),改轮询,给够上限(3s)但不空等。
    let pid = poll_until(Duration::from_secs(3), || pgrep_first(&script_path_str)).await;
    let Some(pid) = pid else {
        eprintln!("ASSERT FAILED: 探针脚本进程没能找到(pgrep -f 3s 内未命中),可能没起来");
        let _ = execute.cancel(&mk_cx(), &ticket).await;
        let _ = std::fs::remove_dir_all(&ws);
        let _ = std::fs::remove_dir_all(&script_dir);
        return false;
    };
    let Some(pgid) = ps_pgid(pid) else {
        eprintln!("ASSERT FAILED: 读不到探针脚本(pid={pid})的 pgid");
        let _ = execute.cancel(&mk_cx(), &ticket).await;
        let _ = std::fs::remove_dir_all(&ws);
        let _ = std::fs::remove_dir_all(&script_dir);
        return false;
    };
    // 同样轮询等孙进程(nohup 出去的 sleep)真的挂进这个进程组——脚本自己
    // 起来了不代表它 fork 出的后台 job 也已经起来。
    let before = poll_until(Duration::from_secs(3), || {
        let members = pgrep_group(pgid);
        if members.len() >= 2 {
            Some(members)
        } else {
            None
        }
    })
    .await
    .unwrap_or_else(|| pgrep_group(pgid));
    println!("探针脚本 pid={pid} pgid={pgid},cancel 前进程组成员: {before:?}");
    if before.len() < 2 {
        eprintln!(
            "ASSERT FAILED: cancel 前进程组成员应至少 2 个(脚本本身 + nohup 出去的 sleep 孙进\
             程),实得 {before:?}——可能 nohup 还没来得及起来,复现条件不成立"
        );
        ok = false;
    } else {
        println!("OK: 进程组里真有孙进程(nohup 出去的 sleep),复现条件成立");
    }

    match execute.poll(&mk_cx(), &ticket).await {
        Ok(ok_val) => match ok_val.value {
            ExecState::Running => println!("OK: cancel 前 poll -> Running"),
            other => {
                eprintln!(
                    "ASSERT FAILED: cancel 前应仍是 Running,实得 {other:?}(脚本可能提早退出了)"
                );
                ok = false;
            }
        },
        Err(f) => {
            eprintln!("ASSERT FAILED: poll 不应该失败:{}", f.err);
            ok = false;
        }
    }

    if let Err(f) = execute.cancel(&mk_cx(), &ticket).await {
        eprintln!("ASSERT FAILED: cancel 不应该失败:{}", f.err);
        ok = false;
    }

    // killpg_graceful 内含 200ms SIGHUP 宽限 + spawn_blocking 的
    // child.wait()——轮询等收尾真的跑完(上限 5s),不是靠猜一个固定
    // sleep。
    let after = poll_until(Duration::from_secs(5), || {
        let members = pgrep_group(pgid);
        if members.is_empty() {
            Some(members)
        } else {
            None
        }
    })
    .await
    .unwrap_or_else(|| pgrep_group(pgid));
    if after.is_empty() {
        println!("OK: cancel 后按进程组(pgid={pgid})断言子孙全不在:{after:?}");
    } else {
        eprintln!("ASSERT FAILED: cancel 后进程组仍有残留成员:{after:?}(取消真杀失守)");
        ok = false;
    }

    let _ = std::fs::remove_dir_all(&ws);
    let _ = std::fs::remove_dir_all(&script_dir);
    ok
}

#[cfg(not(unix))]
async fn section_cancel_real_kill() -> bool {
    println!("== 5. 取消真杀进常绿:跳过(本节靠 nohup/pgrep/ps 等 unix 概念,当前编译目标非 unix)==");
    true
}

/// 每 100ms 跑一次 `check`,拿到 `Some` 就立刻返回;`deadline` 内始终
/// `None` 就返回 `None`。**改用轮询而不是固定 `sleep`**:本轮修复实测复
/// 现过一次假失败——固定 500ms 在系统繁忙时不够长等 `nohup` 真的把孙进程
/// 起来,轮询给够上限但不空等,比猜一个固定时长更稳。`tokio::time::sleep`
/// 而不是 `std::thread::sleep`——这是 async fn,不阻塞 tokio 工作线程。
#[cfg(unix)]
async fn poll_until<T>(deadline: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let start = std::time::Instant::now();
    loop {
        if let Some(v) = check() {
            return Some(v);
        }
        if start.elapsed() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `pgrep -f <pattern>` 取第一个命中的 pid(命令行子串匹配)。找不到 ->
/// `None`。只用于本节验证,不是产品代码。
#[cfg(unix)]
fn pgrep_first(pattern: &str) -> Option<u32> {
    let output = std::process::Command::new("pgrep")
        .arg("-f")
        .arg(pattern)
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|l| l.trim().parse().ok())
}

/// `ps -o pgid= -p <pid>` 读一个 pid 所在的进程组号。
#[cfg(unix)]
fn ps_pgid(pid: u32) -> Option<u32> {
    let output = std::process::Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// `pgrep -g <pgid>` 列出整个进程组当前存活的成员 pid——`killpg` 后应为空。
#[cfg(unix)]
fn pgrep_group(pgid: u32) -> Vec<u32> {
    let Ok(output) = std::process::Command::new("pgrep")
        .args(["-g", &pgid.to_string()])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

// ─── 6. --real(可选,不进常绿门禁)────────────────────────────────────

/// 真实 claude 会话尝试一次(design §7.3):临时 git 工作区 + 真实
/// `InteractiveCliExecutor`(经 `pty_backend` 真起 PTY 子进程)→ 轮询到结束
/// 或超时 → 读回 `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl` 存在且非
/// 空。**成败都打印,不改变 `main` 里其余四节聚合出的退出码**——真实
/// claude 受 GLM 网关抖动影响,只在人手/监理脚本里跑,可安全重试。
async fn section_real() {
    println!("== 5. --real:真实 claude 会话尝试一次(留档,不进常绿门禁)==");

    let ws = mk_git_workspace("main");
    println!("临时工作区: {}", ws.display());

    let binding = ProjectBinding {
        project: ProjectId::new(),
        host: String::new(),
        path: "agent-session-real-demo".into(),
    };
    let real_exec = Arc::new(InteractiveCliExecutor::new());
    let conn = AgentCliConnector::new(binding, &CLAUDE, real_exec);
    let execute = conn.as_execute().expect("应支持 Execute");

    let cx = mk_cx();
    let req = cx.req;
    let spec = ExecSpec {
        workspace: ws.clone(),
        branch: "main".into(),
        inject: vec![InjectBlock {
            label: "demo".into(),
            body: "这是 agent_session --real 档的演示注入块,不是真实技能库内容。".into(),
        }],
        budget_usd: None,
    };
    let ticket = match execute.start(&cx, spec).await {
        Ok(ok_val) => ok_val.value,
        Err(f) => {
            eprintln!(
                "REAL_RUN_FAILED(start 阶段,可能是网关抖动或环境问题):{}",
                f.err
            );
            let _ = std::fs::remove_dir_all(&ws);
            return;
        }
    };
    println!(
        "start 成功,upstream_session={:?},开始轮询……",
        ticket.upstream_session
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    let mut final_state = None;
    while std::time::Instant::now() < deadline {
        let cx_poll = mk_cx();
        match execute.poll(&cx_poll, &ticket).await {
            Ok(ok_val) => {
                if let ExecState::Finished { .. } = &ok_val.value {
                    final_state = Some(ok_val.value);
                    break;
                }
            }
            Err(f) => {
                eprintln!("REAL_RUN_FAILED(poll 阶段):{}", f.err);
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    match final_state {
        Some(state) => println!("REAL_RUN 会话结束: {state:?}"),
        None => println!("REAL_RUN 90s 内未结束(可能是网关抖动/真实交互耗时),取消并如实标记未完成"),
    }
    let cx_cancel = mk_cx();
    let _ = execute.cancel(&cx_cancel, &ticket).await;

    if let Some(upstream) = &ticket.upstream_session {
        if let Some((home_hint, found)) = check_upstream_jsonl(&ws, upstream) {
            println!(
                "上游 jsonl 读回:{home_hint} · 存在={} · 会话号={upstream}",
                found
            );
        }
    }

    if let Some(row) = conn.session_row(req) {
        println!("会话行读回:injected={:?}", row.injected);
    }

    let _ = std::fs::remove_dir_all(&ws);
}

/// 读回 `~/.claude/projects/<encoded-cwd>/<upstream>.jsonl` 是否存在且非
/// 空——只读文件名/大小,不解析内容(同 `discover_sessions` 的口径)。
fn check_upstream_jsonl(workspace: &Path, upstream: &str) -> Option<(String, bool)> {
    let home = std::env::var_os("HOME")?;
    let encoded: String = workspace
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    let path = PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(encoded)
        .join(format!("{upstream}.jsonl"));
    let found = std::fs::metadata(&path)
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    Some((path.display().to_string(), found))
}
