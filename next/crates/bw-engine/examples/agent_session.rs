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
//! 五节,对应 design §7.1「四件要证的事」+ §7.2「类型断路」:
//! 1. **注册表机械证明**(§5.3):claude/cursor 两行各跑一次计划构造,claude
//!    的装配参数与 v1 逐字节一致;cursor 如实拒绝——证明表驱动的分叉真的
//!    走通了,不是碰巧只有一条路。
//! 2. **类型断路**(§7.2 第 5 条):`ExecState` 穷举匹配,证明没有 `Done`
//!    档——编译期证明,不是运行时断言。
//! 3. **会话反查反证**(§7.1「续接真同路径」的反证部分):`discover_sessions`
//!    只认同路径,换一个路径找不到会话——只读文件系统,`BW_CLAUDE_HOME`
//!    指向 scratch 目录,不碰用户真实 `~/.claude/projects/`。
//! 4. **mock 生命周期**(§7.2 第 1-4/6 条 + 两条故意失败路径):
//!    `AgentCliConnector` + `MockInteractiveExecutor`,证明「只起不等 /
//!    续接自动走 resume 计划 / 取消真触发收尾且不被后台事后完成覆盖 / 注
//!    入清单与落盘系统提示词一致 / 工作区不存在或分支不符如实拒绝」。
//! 5. **`--real`(可选,不进常绿门禁)**:真实 `claude` 会话尝试一次,读回
//!    `~/.claude/projects/<encoded>/<uuid>.jsonl` 存在且非空。网关抖动是已
//!    知常态,失败可安全重试;成败都打印,不影响其余四节的断言结果。
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
    MockInteractiveExecutor, PtyInput, SkillOutput, CLAUDE, CURSOR,
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

    // -- claude · 首启 --
    let position_prompt = "示例位置 prompt(这件活要做什么)";
    let system_prompt = "示例系统提示词(衔接层正文,占位)";
    match build_startup_plan(&CLAUDE, position_prompt, system_prompt, &tmp) {
        Ok(plan) => {
            println!("claude 首启装配:{} {}", plan.binary, plan.args.join(" "));
            let expected = vec![
                "--append-system-prompt".to_string(),
                system_prompt.to_string(),
                position_prompt.to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--disallowedTools".to_string(),
                "Bash(gh pr merge)".to_string(),
            ];
            ok &= assert_eq_vec("claude 首启参数", &plan.args, &expected);
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: claude(verified=true)首启装配不应该失败,实得:{e}");
            ok = false;
        }
    }

    // -- claude · 首启,system_prompt 为空(design §5.2「如实降级」)--
    match build_startup_plan(&CLAUDE, position_prompt, "", &tmp) {
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
    match build_startup_plan(&CURSOR, position_prompt, system_prompt, &tmp) {
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
/// 一个从没出现过的路径反查,断言为空。全程只碰 `BW_CLAUDE_HOME` 指向的
/// scratch 目录,不碰用户真实的 `~/.claude/projects/`。
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
    let (expected_prompt, expected_records) = assemble_system_prompt(&inject_blocks);
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
            if row.budget_enforced {
                eprintln!(
                    "ASSERT FAILED: claude 交互式接不上按次预算封顶,budget_enforced 应为 false"
                );
                ok = false;
            } else {
                println!("OK: budget_enforced=false(如实无视 budget_usd,已打诊断行)");
            }
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
            ExecState::Finished { ended, .. } => {
                println!("OK: poll 报 Finished({ended:?})——mock 已完成")
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
        workspace: ws2,
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

    ok
}

// ─── 5. --real(可选,不进常绿门禁)────────────────────────────────────

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
