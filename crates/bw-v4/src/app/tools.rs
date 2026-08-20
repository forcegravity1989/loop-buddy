//! 开工工具:映射保存与探活。
//!
//! 探不到就说探不到。**没接实现的工具返回「不知道」**,不报绿也不报红——项目墙
//! 那条「测一下」上,灰项和红项是两件不同的事。

use super::{App, Result};
use crate::command::{Event, ProbeResult};
use crate::model::{category_key, Category, ProjectId};
use crate::repo::issue_policy_file::{self, CategoryMapping};

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

    pub(super) async fn probe_tool(&mut self, name: String) -> Result<Vec<Event>> {
        let result = probe(&name).await;
        Ok(vec![Event::ToolProbed { name, result }])
    }
}

async fn probe(name: &str) -> ProbeResult {
    match name {
        // Claude CLI 沿用既有的候选路径探测(Windows 上要认 `claude.cmd`)。
        "claude_cli" => match bw_engine::resolve_claude_binary(None) {
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
    let out = bw_engine::tokio_cmd(bin)
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
