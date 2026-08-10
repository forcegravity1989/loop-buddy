//! 脚本采集连接器适配器(feature = "script")。项目仓里的采集脚本
//! (`.bw/connectors.toml` 正本,解析见 `crate::adapters::script_source`)——
//! 唯一一家本片首发收编的连接器,没有 v1 冻结上游函数体可整体搬(v1 的脚本
//! 执行逻辑内嵌在 `bw-app::collect_project_metrics` 里,和 store/指标域深度
//! 耦合,不是一段能直接挪的自由函数),因此本文件是**新写**,不是搬迁——
//! 语义照 v1(只读 `output` 文件、丢弃 stdout;`command` 空则默认
//! `python`;脚本路径不许绝对路径),但取消/超时义务由本文件自己兑现
//! (`.kill_on_drop(true)`,design §4 对新写适配器的要求,gh/codehub 两家
//! 冻结上游没有这条义务)。
//!
//! **Probe 比 v1 更严格**(design §5 定的口径,brief 复述):v1
//! `collect_project_metrics` 的探活只检查脚本文件在位;这里额外验证脚本
//! **可执行**(unix 权限位;非 unix 平台无对应概念,视存在即可执行)与输出
//! 目录**可写**(真写一个探测文件再删掉,不是只读权限位——"装了≠连上了"的
//! 严格口径延伸到脚本连接器:光文件在位不代表采集跑得动)。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use crate::caps::{Collect, CollectOut, CollectReq, Connector, Probe, ProbeReport};
use crate::contract::{
    guarded, unsupported, CallCtx, Capability, ConfigRef, ConnError, ConnResult, ConnectorEntry,
    ConnectorKind, OpClass, ProjectBinding,
};

/// 脚本连接器。`binding.host` 恒 `""`(脚本连接器没有远端 host 概念);
/// `binding.path` = 工作区根绝对路径。`script`/`command`/`output` 三字段
/// 直接对应 `.bw/connectors.toml` 的同名字段(`ConfigRef::Script` 展开存,
/// 避免每次调用都要 match 一次 `config`)。
pub struct ScriptConnector {
    kind: ConnectorKind,
    binding: ProjectBinding,
    script: String,
    command: String,
    output: String,
}

impl ScriptConnector {
    pub fn new(binding: ProjectBinding, script: String, command: String, output: String) -> Self {
        Self {
            kind: ConnectorKind::Script,
            binding,
            script,
            command,
            output,
        }
    }

    /// 登记工厂用的构造入口。**不经共享的 [`crate::adapters::from_entry`]**
    /// ——那个函数明文只分派仓连接器(design 裁决 #2 附带说明,`adapters/
    /// mod.rs` 文档同款):composition root 对 `Script` 登记直接调这里,
    /// 一个项目可有多条脚本登记,天然要循环调用,不是「找一条工厂」的形状。
    pub fn from_entry(entry: &ConnectorEntry) -> Arc<dyn Connector> {
        assert_eq!(
            entry.kind,
            ConnectorKind::Script,
            "ScriptConnector::from_entry 收到非 Script 登记(name={:?},kind={:?})——\
             composition root 的装配期编码错误",
            entry.name,
            entry.kind
        );
        let ConfigRef::Script {
            script,
            command,
            output,
        } = &entry.config
        else {
            panic!(
                "ScriptConnector::from_entry 收到 kind=Script 但 config 不是 \
                 ConfigRef::Script(name={:?})——composition root 的装配期编码错误",
                entry.name
            );
        };
        Arc::new(Self::new(
            entry.binding.clone(),
            script.clone(),
            command.clone(),
            output.clone(),
        ))
    }

    fn workspace(&self) -> PathBuf {
        PathBuf::from(&self.binding.path)
    }
}

impl Connector for ScriptConnector {
    fn kind(&self) -> &ConnectorKind {
        &self.kind
    }

    fn binding(&self) -> &ProjectBinding {
        &self.binding
    }

    fn as_probe(&self) -> Option<&dyn Probe> {
        Some(self)
    }

    fn as_collect(&self) -> Option<&dyn Collect> {
        Some(self)
    }
}

/// unix 上真查可执行权限位;非 unix(Windows)没有对应的 POSIX x-bit 概念
/// ——脚本本来就是被解释器读取执行,不是直接 exec,存在即视为可执行,
/// 与 v1 探活口径(只查文件在位)在 Windows 上保持一致。
#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    true
}

/// 真写一个探测文件再删掉——不是只读权限位。权限位在某些文件系统/挂载模式
/// 下会撒谎(只读快照挂载出的目录权限位可能仍显示可写),真写一次才是诚实
/// 的"连上了"。探测文件名带随机 uuid,避免并发探活互相冲突/落地残留同名文件
/// 被误判。
fn probe_writable(dir: &Path) -> std::io::Result<()> {
    std::fs::metadata(dir)?;
    let probe_path = dir.join(format!(".bw-connector-probe-{}", uuid::Uuid::new_v4()));
    std::fs::write(&probe_path, b"probe")?;
    std::fs::remove_file(&probe_path)?;
    Ok(())
}

/// 探活的同步部分(纯文件系统检查,不 shell-out,不需要 `guarded` 之外的
/// 额外超时——本身就是毫秒级操作)。
fn probe_script(
    workspace: &Path,
    script_rel: &str,
    output_rel: &str,
) -> Result<ProbeReport, ConnError> {
    let script_rel = script_rel.trim();
    if script_rel.is_empty() {
        return Err(ConnError::NotConnected("未记录脚本路径".into()));
    }
    if Path::new(script_rel).is_absolute() {
        // design §5:「绝对路径脚本 → ConnError::UpstreamRejected(v1 已有此
        // 校验,原样保留)」——v1 `collect_project_metrics`/探活分支都是
        // `summary.failed += 1`/`Ok(false, …)`,这里归一化到 UpstreamRejected
        // (连接器本身配置不合规,不是"没连上"这类环境问题)。
        return Err(ConnError::UpstreamRejected {
            message: format!("script 路径 {script_rel} 是绝对路径,需相对工作区"),
        });
    }
    let output_rel_trim = output_rel.trim();
    if output_rel_trim.is_empty() {
        // design §5 明文:「output 为空 → 直接 ConnError::NotConnected…在探
        // 活阶段就说清,不等到采集时静默失败」(2026-08-06 真实事故的教训)。
        return Err(ConnError::NotConnected(
            "未配置 output,采集必然采不到".into(),
        ));
    }

    let script_path = workspace.join(script_rel);
    let meta = std::fs::metadata(&script_path).map_err(|e| {
        ConnError::NotConnected(format!("脚本 {} 不存在:{e}", script_path.display()))
    })?;
    if !meta.is_file() {
        return Err(ConnError::NotConnected(format!(
            "脚本 {} 不是普通文件",
            script_path.display()
        )));
    }
    if !is_executable(&meta) {
        return Err(ConnError::NotConnected(format!(
            "脚本 {} 无可执行权限",
            script_path.display()
        )));
    }

    let output_path = workspace.join(output_rel_trim);
    let output_dir = output_path.parent().unwrap_or(workspace);
    probe_writable(output_dir).map_err(|e| {
        ConnError::NotConnected(format!("输出目录 {} 不可写:{e}", output_dir.display()))
    })?;

    Ok(ProbeReport {
        identity: script_path.display().to_string(),
        detail: format!(
            "脚本 {script_rel} 在位且可执行 · 输出目录 {} 可写",
            output_dir.display()
        ),
    })
}

#[async_trait]
impl Probe for ScriptConnector {
    async fn probe(&self, cx: &CallCtx) -> ConnResult<ProbeReport> {
        let workspace = self.workspace();
        let script = self.script.clone();
        let output = self.output.clone();
        guarded(cx, OpClass::Probe, async move {
            probe_script(&workspace, &script, &output)
        })
        .await
    }
}

/// 截取字符串末尾至多 `n` 个字符(v1 `collect_project_metrics` 截 stderr
/// 尾部 500 字符同款做法,避免长崩溃堆栈把错误消息撑爆)。
fn tail_chars(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        chars[chars.len() - n..].iter().collect()
    }
}

/// 解释器候选链——**有意简化过的 v1 语义**(见本文件模块文档「Probe 比 v1
/// 更严格」旁边这条对应的「Collect 比 v1 简化」):v1
/// `script_interpreter_candidates` 是为 Windows 上跑 `.sh` 脚本准备的候选链
/// (bash/sh.exe/bash.exe + 从 `git.exe` 反推安装路径 + 几个常见 Git for
/// Windows 安装位硬编码兜底),服务的是"脚本用 bash/sh 解释"这个次要场景。
/// 这里只保留其中命中率最高、真实事故验证过的一条:`python`/`python3` 在
/// Windows 上找不到时退而求其次试 `py`(python.org 安装器默认装的启动器,
/// 2026-08-06 真实事故的直接教训)。**bash/git.exe 那条候选链本片未移植**
/// ——记在报告 concerns 里,不是漏做,是范围裁剪:那条链服务的是脚本连接器
/// 里相对少见的 bash 脚本场景,而且需要额外解析 `git --exec-path` 这类
/// 探测逻辑,和这一片"连接器地基"的重量不成比例。
fn interpreter_candidates(command: &str) -> Vec<String> {
    let mut v = vec![command.to_string()];
    if cfg!(windows) && matches!(command, "python" | "python3") {
        v.push("py".to_string());
    }
    v
}

/// 跑一次脚本 → 只读 `output` 文件(丢弃 stdout,2026-08-06 真实事故的老规
/// 矩)→ 交回整份 JSON。**不在这里做超时**——`guarded` 包装器统一兜
/// (`OpClass::CollectScript` 默认 180s),这个函数只管业务逻辑本身。
async fn run_script(
    workspace: &Path,
    script_rel: &str,
    command_cfg: &str,
    output_rel: &str,
) -> Result<CollectOut, ConnError> {
    let script_rel = script_rel.trim();
    if script_rel.is_empty() {
        return Err(ConnError::NotConnected("未记录脚本路径".into()));
    }
    if Path::new(script_rel).is_absolute() {
        return Err(ConnError::UpstreamRejected {
            message: format!("script 路径 {script_rel} 是绝对路径,需相对工作区"),
        });
    }
    let output_rel_trim = output_rel.trim();
    if output_rel_trim.is_empty() {
        return Err(ConnError::NotConnected(
            "未配置 output,采集必然采不到".into(),
        ));
    }

    let command = if command_cfg.trim().is_empty() {
        "python".to_string()
    } else {
        command_cfg.trim().to_string()
    };
    let script_path = workspace.join(script_rel);
    let candidates = interpreter_candidates(&command);

    let mut ran = false;
    let mut last_err: Option<ConnError> = None;
    for cand in &candidates {
        let mut cmd = tokio::process::Command::new(cand);
        cmd.arg(&script_path)
            .current_dir(workspace)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // 新写适配器必须兑现的取消义务(design §4 / `guarded` 文档):
            // `guarded` 落选分支 drop 这个 future 时,子进程要真的被杀掉。
            .kill_on_drop(true);
        match cmd.output().await {
            Ok(o) if o.status.success() => {
                ran = true;
                last_err = None;
                break;
            }
            Ok(o) => {
                // 此候选能 spawn(脚本能跑),非零退出——用这个结果,不试下
                // 一个候选(同 v1:候选链只为解决"解释器不在 PATH",不是为
                // 了在多个真实存在的解释器之间找一个能跑通的)。
                let stderr = String::from_utf8_lossy(&o.stderr);
                last_err = Some(ConnError::UpstreamRejected {
                    message: format!(
                        "script {script_rel} 非零退出({cand}):{}",
                        tail_chars(stderr.trim_end(), 500)
                    ),
                });
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                last_err = Some(ConnError::NotConnected(format!("解释器 {cand} 不在 PATH")));
                continue;
            }
            Err(e) => {
                last_err = Some(ConnError::Other(format!(
                    "script {script_rel} spawn 失败({cand}):{e}"
                )));
                break;
            }
        }
    }
    if !ran {
        return Err(last_err.unwrap_or_else(|| {
            ConnError::NotConnected(format!("无可用解释器(试过 {})", candidates.join("/")))
        }));
    }

    // 只读 output 文件,丢弃 stdout——2026-08-06 真实事故的老规矩:脚本只
    // print 到 stdout、output 留空,指标永远 Unknown。
    let output_path = workspace.join(output_rel_trim);
    let raw = std::fs::read_to_string(&output_path).map_err(|e| {
        ConnError::NotConnected(format!("脚本输出 {} 读不到:{e}", output_path.display()))
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| ConnError::Unparsable {
            raw: format!("脚本输出 {} 非 JSON:{e}", output_path.display()),
        })?;
    Ok(CollectOut {
        value,
        source_hint: format!("script · {script_rel} · {}", output_path.display()),
    })
}

#[async_trait]
impl Collect for ScriptConnector {
    async fn collect(&self, cx: &CallCtx, req: CollectReq) -> ConnResult<CollectOut> {
        match req {
            // 脚本连接器不做仓计数查询——如实 Unsupported,不给假空结果。
            CollectReq::RemoteCount { .. } => unsupported(cx, Capability::Collect, "remote_count"),
            CollectReq::ScriptRun => {
                let workspace = self.workspace();
                let script = self.script.clone();
                let command = self.command.clone();
                let output = self.output.clone();
                guarded(cx, OpClass::CollectScript, async move {
                    run_script(&workspace, &script, &command, &output).await
                })
                .await
            }
        }
    }
}
