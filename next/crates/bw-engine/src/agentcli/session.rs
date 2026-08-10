//! 会话身份的内存表(next 切片三C,design-s3-agentcli.md §1.2/§2)。
//!
//! **不落任何 BW 自有持久化**——判据是「切片三验收要能证明什么」:要证的是
//! 同路径续接真的拿得到历史,不是「BW 重启后还记得哪件活对应哪个会话」
//! (那要活的存储才谈得上,是切片四/五的事)。上游(claude CLI)已经把会话
//! 存在 `~/.claude/projects/<encoded-cwd>/<session>.jsonl`,BW 再落一份就是
//! 给同一个事实开第二本账(design §2.1 三条理由)。
//!
//! [`SessionTable`] 只活在一次进程运行期间;[`discover_sessions`] 是重启后
//! 找回「这个工作区有哪些会话」的**便利,不是命脉**——续接靠的是显式传给
//! `--resume`/`--session-id` 的那个 uuid,反查只是新进程起来时的一次找回,
//! 上游哪天改了目录编码规则,反查失灵,续接的正向路径不受影响。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bw_connector::{ExecState, RequestId};
use bw_core::ConversationId;

/// 一块注入正文的事实记录(design §6.2「记账口」)。切片三只产事实,不结
/// 算——切片四运行管理器接上存储后,拿这张清单做「同一件活只记一次」的
/// 结算(settle-once)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectRecord {
    pub label: String,
    /// 正文字节数(`body.len()`,UTF-8 字节,不是字符数)。
    pub bytes: usize,
    /// 内容指纹:sha256 前 8 位十六进制字符,够对上「是不是同一块」,不需要
    /// 引入额外的 hex crate——直接取摘要头 4 字节各自格式化两位十六进制。
    pub digest8: String,
}

/// 一次执行 = 一行。字段全部是「能观测到的事实」,没有一个是猜出来的
/// (design §1.2)。
#[derive(Debug, Clone)]
pub struct SessionRow {
    /// 编排层的键,等于这次调用的 `CallCtx.req`(也是回传给调用方的
    /// `ExecTicket.req`)——一次调用一个,不是另起的第二个 id。
    pub req: RequestId,
    /// 界面的键(`terminal_manager::TerminalManager` 按它路由字节)。
    pub conversation: ConversationId,
    /// **续接必须同路径,这是硬事实**(design §2.2)——上游按工作目录编码
    /// 存会话,换路径就找不到历史。
    pub workspace: PathBuf,
    pub branch: String,
    /// 上游会话号。claude 一家可以由我们指派(`--session-id`,已用真实交
    /// 互式会话验证过接受这个旗标),首启就有值;不能指派的家在这里留空,
    /// 如实标注「未知」,绝不填一个猜的。
    pub upstream_session: String,
    /// 这次注入了哪些块(标签/字节数/内容指纹)。**只记事实,不做结
    /// 算**——续接会话的这个字段恒为空(§6.3:复利块首启时已经在会话里,
    /// 重注是污染,不是漏注)。
    pub injected: Vec<InjectRecord>,
    /// 本家 CLI 是否真的接得上按次预算封顶。claude 交互式接不上,如实
    /// `false`(design §8/主控裁决 #3)。
    pub budget_enforced: bool,
    pub state: ExecState,
}

/// 生命周期与身份表。map,不设固定槽(主控裁决 #8:架构上不得内含「同时
/// 只有几个会话」的隐藏假设,plan/23 §2 的百级并行要求)。
///
/// **并发纪律**:调用方(`agentcli::connector::AgentCliConnector`)持有的
/// 是 `Arc<Mutex<SessionTable>>`——拿锁取到要的东西就出来,不得跨 `.await`
/// 持有这把锁(design §1.2)。这条纪律靠调用方遵守,本类型自己不做异步。
#[derive(Default)]
pub struct SessionTable {
    by_req: HashMap<RequestId, SessionRow>,
    /// 「这个工作区已经有会话了吗」——续接判定唯一入口,不给第二条按名查
    /// 找的路。同一路径重复 `start` 会用新的 `req` 覆盖这里的映射,旧的
    /// `req` 仍留在 `by_req` 里(不做 GC——design §12 开放问题 #8 原样继
    /// 承,本片不做闲置回收)。
    by_workspace: HashMap<PathBuf, RequestId>,
}

impl SessionTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记/覆盖一行。`workspace` 相同的旧行仍留在 `by_req` 里(不删),只
    /// 是 `by_workspace` 的路由改指向新行。
    pub fn insert(&mut self, row: SessionRow) {
        self.by_workspace.insert(row.workspace.clone(), row.req);
        self.by_req.insert(row.req, row);
    }

    pub fn get(&self, req: RequestId) -> Option<&SessionRow> {
        self.by_req.get(&req)
    }

    pub fn get_mut(&mut self, req: RequestId) -> Option<&mut SessionRow> {
        self.by_req.get_mut(&req)
    }

    /// 续接判定唯一入口:这个工作区当前(最近一次 `insert`)对应哪一行。
    pub fn find_by_workspace(&self, workspace: &Path) -> Option<&SessionRow> {
        self.by_workspace
            .get(workspace)
            .and_then(|req| self.by_req.get(req))
    }

    pub fn len(&self) -> usize {
        self.by_req.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_req.is_empty()
    }
}

/// 上游 `~/.claude/projects/` 的目录编码规则——2026-08-10 本机实测反推(不
/// 是查到的官方文档):把 cwd 路径里的 `/` 与 `.` 都换成 `-`,其余字符原样
/// 保留。核对法:`ls ~/.claude/projects/` 直接对比本机真实工作区路径,逐字
/// 符吻合(含 `.claude` 段产生的双 `-`)。上游哪天改了编码方式,
/// [`discover_sessions`] 会找不到东西——但这只影响反查这条**便利**路径,不
/// 影响续接的正向路径(显式传 uuid 给 `--resume`/`--session-id`)。
fn encode_workspace_path(workspace: &Path) -> String {
    workspace
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect()
}

/// `~/.claude/projects/` 的根目录。**`BW_CLAUDE_HOME` 覆盖**(同
/// `BW_CLAUDE_BIN` 的既有约定):指挥器/测试用它指向一个 scratch 目录,不
/// 碰用户真实的 `~/.claude/projects/`;不设时落到真实 `$HOME`(Windows 上
/// 退回 `USERPROFILE`,未在本机验证,如实标注)。
fn claude_projects_dir() -> Option<PathBuf> {
    if let Ok(override_home) = std::env::var("BW_CLAUDE_HOME") {
        return Some(PathBuf::from(override_home).join("projects"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// 反查器:某个工作区路径下,上游存了哪些会话,连同各自的修改时间。**只读
/// 文件名与修改时间,不解析 jsonl 内容**——上游的行格式没有公开文档,解析
/// 器会脆(v1 §13 已记,design §2.1/§8 同口径)。目录不存在(从没在这个路径
/// 起过会话,或 `$HOME`/`BW_CLAUDE_HOME` 解不出来)时如实返回空,不是错误。
pub fn discover_sessions(workspace: &Path) -> Vec<(String, SystemTime)> {
    let Some(dir) = claude_projects_dir() else {
        return Vec::new();
    };
    let session_dir = dir.join(encode_workspace_path(workspace));
    let Ok(entries) = std::fs::read_dir(&session_dir) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(mtime) = meta.modified() else {
            continue;
        };
        out.push((stem.to_string(), mtime));
    }
    out
}
