//! 开工工具:映射保存与探活。
//!
//! 探不到就说探不到。**没接实现的工具返回「不知道」**,不报绿也不报红——项目墙
//! 那条「测一下」上,灰项和红项是两件不同的事。

use super::{App, AppError, Result};
use crate::command::{Event, ProbeResult};
use crate::model::{category_key, Category, ProjectId};
use crate::repo::issue_policy_file::{self, CategoryMapping};
use crate::store::WORKSPACES_ROOT_KEY;

impl App {
    pub(super) async fn save_tool_mapping(
        &mut self,
        project_id: ProjectId,
        category: Category,
        tool: String,
        workflow: String,
    ) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        let mut file = issue_policy_file::read(&ws)?.unwrap_or_else(|| {
            issue_policy_file::parse(crate::standard::ISSUE_POLICY_TMPL).unwrap_or_default()
        });
        let key = category_key(category).to_string();
        match file.mappings.iter_mut().find(|m| m.category == key) {
            Some(m) => {
                m.tool = tool;
                m.workflow = workflow;
            }
            None => file.mappings.push(CategoryMapping {
                category: key,
                tool,
                workflow,
            }),
        }
        issue_policy_file::write(&ws, &file)?;
        Ok(vec![Event::ToolMappingSaved { category }])
    }

    /// 改工作区根目录。存 `app_meta`,进程内立刻生效。
    ///
    /// **已接入的项目一个都不动** —— 但这件事不是自动成立的:接入时没填仓路径的
    /// 项目,库里那一列是空的,`workspace_of` 每次都拿**当下**的根目录现拼
    /// (`<根目录>/<slug>`)。所以改根目录之前必须先把它们**钉死**:把各自当下
    /// 算出来的绝对路径写回库里。不钉的话,改一下根目录,已有项目就集体指到一个
    /// 空目录 —— 仓还在硬盘上,但 buddy 全看不见了(健康变灰、文件树全空)。
    ///
    /// 钉死是一次性的:钉完之后它们和「根目录」再没有关系。**不搬目录** —— 真要
    /// 搬得先解决"仓里有没有没提交的改动、worktree 还开着没有",那是另一件事。
    pub(super) async fn set_workspaces_root(&mut self, path: String) -> Result<Vec<Event>> {
        let path = path.trim().to_string();
        if path.is_empty() {
            return Err(AppError::Refused("工作区根目录不能是空的".into()));
        }
        let p = std::path::PathBuf::from(&path);
        if !p.is_absolute() {
            return Err(AppError::Refused(format!(
                "「{path}」不是绝对路径 —— 相对路径会跟着进程从哪儿启动而变,填全路径"
            )));
        }
        // 建不出来就别存:存了等于把一个用不了的路径留给下一次接入。
        std::fs::create_dir_all(&p)
            .map_err(|e| AppError::Refused(format!("这个目录建不出来:{e}")))?;
        // 先钉死,再换根目录 —— 顺序反了就钉不出老路径了。
        let mut pinned = 0u32;
        for project in self.store.projects().await? {
            if project.workspace_path.trim().is_empty() {
                let here = self.workspaces_root.join(&project.slug);
                self.store
                    .set_project_workspace_path(project.id, &here.display().to_string())
                    .await?;
                pinned += 1;
            }
        }
        self.store.set_meta(WORKSPACES_ROOT_KEY, &path).await?;
        self.workspaces_root = p;
        Ok(vec![Event::WorkspacesRootChanged { path, pinned }])
    }

    pub(super) async fn probe_tool(&mut self, name: String) -> Result<Vec<Event>> {
        let result = probe(&name).await;
        Ok(vec![Event::ToolProbed { name, result }])
    }
}

/// 活上记着的开工工具 → 真正要起的那个 CLI。
///
/// **开工必须照活上记的工具走,不许悄悄换。** 设计正本 §7.9 把这条写死了:
/// 「选了 Cursor 但探活失败 → 开工前置校验直接拒绝报错,**不悄悄退回 Claude
/// CLI** —— 静默换工具会让人以为在用一个工具、实际在用另一个」。这个函数在
/// 2026-08-21 之前不存在,四处开工点全是写死的 `&CLAUDE`,于是人在配置屏选了
/// 别的工具、卡面也挂着那个标签,跑起来的却始终是 claude —— 正是那条规矩要
/// 防的事,而且方向是最坏的那个(默默顶上,不报错)。
///
/// 空值按 Claude CLI 算:活是 buddy 自己建的、还没配过映射时就是空的,
/// 这是默认不是「未知」。
pub fn agent_for(tool: &str) -> Result<&'static v4_engine::TuiAgentConfig> {
    match tool.trim() {
        "" | "claude_cli" => Ok(&v4_engine::CLAUDE),
        // 探活失败与「压根没接」由 `build_startup_plan` 按 `supported` 如实拒。
        "cursor" => Ok(&v4_engine::CURSOR),
        // 本机网页内嵌类,根本不是起一个终端子进程 —— 拿终端那条路去起它,
        // 起出来的一定不是人要的东西,所以这里直接拒,不往下走。
        "open_design" => Err(AppError::Exec(
            "这张活配的开工工具是 Open Design,而内嵌 Open Design 还没接上。\
             要用 Claude CLI 干这张活,先在活详情里把开工工具改掉。"
                .into(),
        )),
        other => Err(AppError::Exec(format!(
            "活上记着的开工工具是「{other}」,buddy 不认识它 —— 不猜、不拿别的工具顶上。\
             去配置屏的「类别 → 开工工具」里改掉,或者在活详情里改这一张。"
        ))),
    }
}

async fn probe(name: &str) -> ProbeResult {
    match name {
        // Claude CLI 沿用既有的候选路径探测(Windows 上要认 `claude.cmd`)。
        "claude_cli" => match v4_engine::resolve_claude_binary(None) {
            Some(p) => ProbeResult::Found(p),
            None => ProbeResult::Missing("本机路径里找不到 claude".into()),
        },
        "cursor" => match version_of("agent", &["--version"]).await {
            Some(v) => ProbeResult::Found(v),
            None => ProbeResult::Missing("本机找不到 cursor 的 agent 二进制".into()),
        },
        "open_design" => match tcp_alive("127.0.0.1:5173").await {
            true => ProbeResult::Found("本机 127.0.0.1:5173 有服务在听".into()),
            false => ProbeResult::Missing("本机 Open Design 没起来".into()),
        },
        // 内部同事在做的群工具。留着位置,如实说「还没接」。
        "welink_cli" => ProbeResult::Unknown("welink-cli 还没接,探不出结果".into()),
        other => ProbeResult::Unknown(format!("没有 {other} 的探活实现")),
    }
}

async fn version_of(bin: &str, args: &[&str]) -> Option<String> {
    let out = v4_engine::tokio_cmd(bin)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string()
    })
}

async fn tcp_alive(addr: &str) -> bool {
    tokio::time::timeout(
        std::time::Duration::from_millis(300),
        tokio::net::TcpStream::connect(addr),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}
