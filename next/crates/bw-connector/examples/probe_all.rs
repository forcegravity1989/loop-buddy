//! `probe_all` —— 切片二行为验收指挥器(next 切片二C)。不开界面,直接调用
//! `bw-connector` 的公开 API,证明「三家连接器真的能连、真的会如实报连不
//! 上」——这是切片二的行为验收件,不是单元测试。
//!
//! **登记来源不依赖数据库**(next/ 尚无 store):命令行给一个项目根目录,从
//! `<root>/.bw/connectors.toml` 读脚本登记(`script_source::read`);另接受
//! `--gh <owner/repo>` / `--codehub <group/proj>` 显式绑定造 gh/codehub 的
//! `ConnectorEntry`(本机没有一个能读出「这个项目该连哪个仓」的数据库,只能
//! 靠命令行显式指定)。
//!
//! 行为:注册 → 逐条**并发**探活(拿 [`ConnectorRegistry::probes`] 的持有型
//! 结果,`tokio::spawn` 出去——这就是这条路由要交回 `Arc<dyn Connector>` 而
//! 不是借用切片的原因:借用做不到 `'static`,并发不起来)→ 打印
//! 「种类 · 名称 · 身份 · 能力集 · 探活结果」,失败按契约七类错误分类如实
//! 打印(绝不把探不通打成绿)。script 连接器额外跑一次真实 `Collect`,证明
//! 「跑脚本 → 只读 output 文件 → 交回 JSON」这条管线真的活着,不是只有
//! Probe 打勾。
//!
//! **必须含一条故意失败路径**(`--with-broken`):注册一条指向不存在脚本文件
//! 的 Script 登记,断言它的探活结果落 `ConnError::NotConnected` 而不是绿——
//! 这是防伪证明,不是可选项。brief 原话是「指向不存在二进制」;这里选
//! Script kind 而不是 gh/codehub 的理由见下方 `register_broken_entry` 的文
//! 档注释。
//!
//! 跑法:
//! ```text
//! cargo run -p bw-connector --example probe_all -- <项目根> [--with-broken] \
//!     [--gh <owner/repo>] [--codehub <group/proj>]
//! ```
//! 退出码 0 且末行 `PROBE_ALL_OK` = 全部断言通过(没传 `--with-broken` 时,
//! 断言集合是空的,空断言集合视为「全部」通过——这不是放水,是这个指挥器
//! 唯一钉死的断言就是「故意失败路径必须落 NotConnected」,没有这条路径就
//! 没有断言可言)。

use std::path::PathBuf;
use std::process::ExitCode;

use bw_connector::{
    CallCtx, Capability, CapabilitySet, ConfigRef, ConnError, ConnectorEntry, ConnectorKind,
    ConnectorRegistry, ProjectBinding, RequestId,
};
use bw_core::{ConnectorId, ProjectId};
use tokio_util::sync::CancellationToken;

/// 故意失败登记的 name——探活结果扫描时用它认出哪一条是「该失败的那条」。
const BROKEN_ENTRY_NAME: &str = "__probe_all_broken_demo__";

struct Args {
    root: PathBuf,
    with_broken: bool,
    gh: Option<String>,
    codehub: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut argv = std::env::args().skip(1);
    let mut root = None;
    let mut with_broken = false;
    let mut gh = None;
    let mut codehub = None;
    while let Some(a) = argv.next() {
        match a.as_str() {
            "--with-broken" => with_broken = true,
            "--gh" => gh = Some(argv.next().ok_or("--gh 需要一个 owner/repo 参数")?),
            "--codehub" => codehub = Some(argv.next().ok_or("--codehub 需要一个 group/proj 参数")?),
            other if root.is_none() => root = Some(PathBuf::from(other)),
            other => return Err(format!("未知参数或重复的项目根:{other}")),
        }
    }
    Ok(Args {
        root: root.ok_or("缺少项目根目录参数")?,
        with_broken,
        gh,
        codehub,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("参数错误:{e}");
            eprintln!(
                "用法:probe_all <项目根> [--with-broken] [--gh <owner/repo>] [--codehub <group/proj>]"
            );
            return ExitCode::FAILURE;
        }
    };

    let project = ProjectId::new();
    let mut registry = ConnectorRegistry::default();

    println!("== 登记 ==");
    println!("项目根: {}", args.root.display());
    println!("项目 id(本次现铸,next/ 无 store): {:?}", project.uuid());

    #[cfg(feature = "gh")]
    if let Some(owner_repo) = &args.gh {
        register_gh(&mut registry, project, owner_repo);
    }
    #[cfg(not(feature = "gh"))]
    if args.gh.is_some() {
        eprintln!("--gh 需要 gh feature,当前构建未开启,忽略");
    }

    #[cfg(feature = "codehub")]
    if let Some(group_proj) = &args.codehub {
        register_codehub(&mut registry, project, group_proj);
    }
    #[cfg(not(feature = "codehub"))]
    if args.codehub.is_some() {
        eprintln!("--codehub 需要 codehub feature,当前构建未开启,忽略");
    }

    #[cfg(feature = "script")]
    {
        match bw_connector::script_source::read(project, &args.root) {
            Ok(entries) => {
                for entry in entries {
                    println!(
                        "+ script · {} · 脚本={:?}",
                        entry.name,
                        match &entry.config {
                            ConfigRef::Script { script, .. } => script.as_str(),
                            _ => "?",
                        }
                    );
                    let conn = bw_connector::ScriptConnector::from_entry(&entry);
                    registry.register(entry, conn);
                }
            }
            Err(e) => {
                eprintln!("ASSERT FAILED: <root>/.bw/connectors.toml 读取/解析失败:{e}");
                return ExitCode::FAILURE;
            }
        }

        if args.with_broken {
            register_broken_entry(&mut registry, project, &args.root);
        }
    }
    #[cfg(not(feature = "script"))]
    if args.with_broken {
        eprintln!("--with-broken 需要 script feature,当前构建未开启,断言无法执行");
        return ExitCode::FAILURE;
    }

    println!();
    println!("== 探活(并发)==");

    let mut handles = Vec::new();
    for (entry, conn) in registry.probes(project) {
        let caps = conn.capabilities();
        handles.push(tokio::spawn(async move {
            let probe = conn
                .as_probe()
                .expect("registry.probes 只返回支持探活的连接器,as_probe 恒 Some");
            let cx = CallCtx {
                req: RequestId::new(),
                timeout: None,
                cancel: CancellationToken::new(),
            };
            let outcome = probe.probe(&cx).await;
            (entry, caps, outcome)
        }));
    }

    let mut broken_seen = false;
    let mut broken_ok = true;
    for h in handles {
        let (entry, caps, outcome) = h.await.expect("探活任务 join 不应 panic");
        print_probe_line(&entry, caps, &outcome);

        if entry.name == BROKEN_ENTRY_NAME {
            broken_seen = true;
            broken_ok = match &outcome {
                Ok(ok) => {
                    eprintln!(
                        "ASSERT FAILED: 故意失败路径探活居然成功(假绿):identity={:?} detail={:?}",
                        ok.value.identity, ok.value.detail
                    );
                    false
                }
                Err(fail) => match &fail.err {
                    ConnError::NotConnected(_) => true,
                    other => {
                        eprintln!(
                            "ASSERT FAILED: 故意失败路径落到了 {other:?},不是 ConnError::NotConnected"
                        );
                        false
                    }
                },
            };
        }
    }

    println!();
    println!("== 采集(script only,证明 Collect 管线真的活着)==");
    #[cfg(feature = "script")]
    run_script_collections(&registry, project).await;
    #[cfg(not(feature = "script"))]
    println!("(script feature 未开启,跳过)");

    println!();
    let mut all_ok = true;
    if args.with_broken {
        if !broken_seen {
            eprintln!("ASSERT FAILED: --with-broken 已传但探活结果里没见到故意失败登记");
            all_ok = false;
        }
        all_ok &= broken_ok;
    } else {
        println!("(未传 --with-broken,跳过防伪断言)");
    }

    if all_ok {
        println!("PROBE_ALL_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("PROBE_ALL_FAILED");
        ExitCode::FAILURE
    }
}

#[cfg(feature = "gh")]
fn register_gh(registry: &mut ConnectorRegistry, project: ProjectId, owner_repo: &str) {
    let entry = ConnectorEntry {
        id: ConnectorId::new(),
        name: "gh".into(),
        kind: ConnectorKind::GithubRepo,
        binding: ProjectBinding {
            project,
            host: "github.com".into(),
            path: owner_repo.to_string(),
        },
        config: ConfigRef::CliLogin { bin: "gh".into() },
    };
    println!("+ gh · {owner_repo}");
    let conn = bw_connector::from_entry(&entry).expect("gh feature 已开,from_entry 必给 Some");
    registry.register(entry, conn);
}

/// codehub host 没有专门的命令行参数(brief 只给 `--codehub <group/proj>`
/// 一种形式)。design §2:codehub 的 `binding.host` 是 API host 别名
/// ("open"/"green"/"yellow"),这里选 `"open"` 当默认——`codehub-cli` 本机
/// 未装(见 `upstream::codehub` 模块文档「2026-08-10 现状」),这条路径在
/// 本机必然落 `NotConnected`,host 具体取哪个别名不影响这次验收的真实性,
/// 只是让 `ConnectorEntry` 有个合法值可注册。
#[cfg(feature = "codehub")]
fn register_codehub(registry: &mut ConnectorRegistry, project: ProjectId, group_proj: &str) {
    let entry = ConnectorEntry {
        id: ConnectorId::new(),
        name: "codehub".into(),
        kind: ConnectorKind::CodehubRepo,
        binding: ProjectBinding {
            project,
            host: "open".into(),
            path: group_proj.to_string(),
        },
        config: ConfigRef::CliLogin {
            bin: "codehub-cli".into(),
        },
    };
    println!("+ codehub · open/{group_proj}");
    let conn = bw_connector::from_entry(&entry).expect("codehub feature 已开,from_entry 必给 Some");
    registry.register(entry, conn);
}

/// 故意失败登记。**brief 原话是「指向不存在二进制」**;这里选 `Script`
/// kind 而不是 `GithubRepo`/`CodehubRepo`,理由三条:
///
/// 1. `gh`/`codehub-cli` 的可执行名目前**硬编码**在冻结的 v1 上游函数体内
///    (`"gh"`/`"codehub-cli"`,见 `crate::contract::ConfigRef::CliLogin` 文档
///    「2026-08-10 现状」)——`ConfigRef::CliLogin.bin` 字段现阶段不被两家适
///    配器消费,没有旋钮能让它们真的去调一个不存在的二进制名。
/// 2. 本机 `gh` 已装且已登录(见本次验收记录)——绑一个不存在的 `owner/repo`
///    只会让 `gh repo view` 真的连上 GitHub、如实回「仓不存在」,那是
///    `ConnError::UpstreamRejected`(连上了、上游说不行),不是
///    `ConnError::NotConnected`(装了≠连上了那一档)——落错分类,不满足
///    brief 要的「NotConnected 类失败」。
/// 3. `Script` 连接器的 Probe 恰好也检查「目标可执行物在不在位」(脚本文件
///    是否存在),指向一个真实不存在的脚本路径,得到的正是
///    `ConnError::NotConnected`——语义上与「指向不存在二进制」等价(「登记
///    了却连不上实际产出物」),而且完全在本次收编的代码路径内,不依赖外部
///    环境的偶然状态(gh 是否装、是否登录、网络是否通)。
#[cfg(feature = "script")]
fn register_broken_entry(
    registry: &mut ConnectorRegistry,
    project: ProjectId,
    root: &std::path::Path,
) {
    let entry = ConnectorEntry {
        id: ConnectorId::new(),
        name: BROKEN_ENTRY_NAME.into(),
        kind: ConnectorKind::Script,
        binding: ProjectBinding {
            project,
            host: String::new(),
            path: root.to_string_lossy().to_string(),
        },
        config: ConfigRef::Script {
            script: "__probe_all_never_created__/nonexistent.py".into(),
            command: "python".into(),
            output: "__probe_all_never_created__/out.json".into(),
        },
    };
    println!("+ script(故意失败) · {} · 指向不存在的脚本文件", entry.name);
    let conn = bw_connector::ScriptConnector::from_entry(&entry);
    registry.register(entry, conn);
}

fn caps_label(caps: CapabilitySet) -> String {
    let mut parts = Vec::new();
    for c in [
        Capability::Probe,
        Capability::Execute,
        Capability::Collect,
        Capability::IssueOps,
    ] {
        if caps.has(c) {
            parts.push(c.to_string());
        }
    }
    if parts.is_empty() {
        "(无)".to_string()
    } else {
        parts.join("+")
    }
}

fn identity_label(b: &ProjectBinding) -> String {
    if b.host.is_empty() {
        b.path.clone()
    } else {
        format!("{}/{}", b.host, b.path)
    }
}

fn err_tag(e: &ConnError) -> &'static str {
    match e {
        ConnError::Unsupported { .. } => "Unsupported",
        ConnError::NotConnected(_) => "NotConnected",
        ConnError::Timeout(_) => "Timeout",
        ConnError::Canceled => "Canceled",
        ConnError::UpstreamRejected { .. } => "UpstreamRejected",
        ConnError::Unparsable { .. } => "Unparsable",
        ConnError::Other(_) => "Other",
    }
}

fn print_probe_line(
    entry: &ConnectorEntry,
    caps: CapabilitySet,
    outcome: &bw_connector::ConnResult<bw_connector::ProbeReport>,
) {
    let kind = format!("{:?}", entry.kind);
    let ident = identity_label(&entry.binding);
    let caps_str = caps_label(caps);
    match outcome {
        Ok(ok) => println!(
            "{kind} · {} · {ident} · 能力[{caps_str}] · 探活: OK · {} (耗时 {:.3}s)",
            entry.name,
            ok.value.detail,
            ok.took.as_secs_f64()
        ),
        Err(fail) => println!(
            "{kind} · {} · {ident} · 能力[{caps_str}] · 探活: FAIL[{}] · {} (耗时 {:.3}s)",
            entry.name,
            err_tag(&fail.err),
            fail.err,
            fail.took.as_secs_f64()
        ),
    }
}

/// 对每一条注册了 Script kind 的连接器真跑一次 `Collect(ScriptRun)`——不是
/// 只测 Probe。探活失败的（比如 `--with-broken` 那条）跳过,不重复报错。
#[cfg(feature = "script")]
async fn run_script_collections(registry: &ConnectorRegistry, project: ProjectId) {
    use bw_connector::CollectReq;

    for (entry, conn) in registry.collectors(project) {
        // 这一节标题写明「script only」——gh/codehub 也实现 Collect(走
        // RemoteCount),但那不是这里要演示的管线;真跑到它们头上只会看见
        // 一堆 Unsupported(它们确实不支持 ScriptRun,如实),没有信息量,
        // 反而会让读输出的人误以为在测别的东西。按 `entry.kind` 收窄。
        if entry.kind != ConnectorKind::Script {
            continue;
        }
        if entry.name == BROKEN_ENTRY_NAME {
            continue; // 故意失败的登记,采集必然也失败,不重复演示。
        }
        let Some(collect) = conn.as_collect() else {
            continue;
        };
        let cx = CallCtx {
            req: RequestId::new(),
            timeout: None,
            cancel: CancellationToken::new(),
        };
        let outcome = collect.collect(&cx, CollectReq::ScriptRun).await;
        match outcome {
            Ok(ok) => println!(
                "script · {} · 采集: OK · {} · value={} (耗时 {:.3}s)",
                entry.name,
                ok.value.source_hint,
                ok.value.value,
                ok.took.as_secs_f64()
            ),
            Err(fail) => println!(
                "script · {} · 采集: FAIL[{}] · {} (耗时 {:.3}s)",
                entry.name,
                err_tag(&fail.err),
                fail.err,
                fail.took.as_secs_f64()
            ),
        }
    }
}
