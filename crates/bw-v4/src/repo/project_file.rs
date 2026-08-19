//! `.bw/project.toml` —— 项目意图与配置的正本。
//!
//! V3 的解析器(`bw_engine::project_file`)只认五个意图字段。V4 多三样:
//! `[chat]` 项目群、`standard_version`(项目用的是哪版规范)、
//! `current_version`(在研版本)。这三样**都不入库**——总览、计划屏每次要显示
//! 就现读现解析这份文件。V4 自己写一份解析器而不是改 V3 那份,是为了让旧壳
//! 一行不动继续可跑。

use super::{parse_toml, read_to_string, write_file, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const REL_PATH: &str = ".bw/project.toml";

/// 项目群配置。不配就整段不写,总览名片显示「未配 · 配置」。
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChatConfig {
    /// 今天只有 `"welink"` 一个真实实现;`"none"` = 明确不配群。
    pub provider: String,
    pub group_id: String,
    /// 同步哪些事件:`review` / `merged` / `release`。
    #[serde(default)]
    pub notify: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub brief: String,
    #[serde(default)]
    pub benchmark: String,
    #[serde(default)]
    pub opportunity: String,
    /// 这个项目用的是哪版规范。与 `.bw/standard.toml` 的 `version` 一致;
    /// 这里是给总览快速展示的镜像,不是第二个正本。
    #[serde(default)]
    pub standard_version: String,
    /// 计划屏顶部可切的「在研版本」。新项目默认 `v0.1`。
    #[serde(default)]
    pub current_version: String,
    #[serde(default)]
    pub chat: Option<ChatConfig>,
}

pub fn parse(raw: &str) -> Result<ProjectFile> {
    parse_toml(REL_PATH, raw)
}

/// 不存在返回 `Ok(None)` —— 诚实的无事发生。
pub fn read(workspace: &Path) -> Result<Option<ProjectFile>> {
    match read_to_string(workspace, REL_PATH)? {
        None => Ok(None),
        Some(raw) => parse(&raw).map(Some),
    }
}

pub fn write(workspace: &Path, file: &ProjectFile) -> Result<String> {
    let body = render(file);
    write_file(workspace, REL_PATH, &body)?;
    Ok(body)
}

/// 手写渲染而不是 `toml::to_string`:注释要留在文件里给人看,`toml` 回写会
/// 把注释全抹掉(这个坑 2026-08-05 在指标文件上踩过一次)。
pub fn render(f: &ProjectFile) -> String {
    let mut s = String::new();
    s.push_str(&format!("name = {}\n", quote(&f.name)));
    s.push_str(&format!("kind = {}\n", quote(&f.kind)));
    s.push_str(&format!("brief = {}\n", quote(&f.brief)));
    s.push_str(&format!("benchmark = {}\n", quote(&f.benchmark)));
    s.push_str(&format!("opportunity = {}\n", quote(&f.opportunity)));
    s.push_str("\n# 规范版本与在研版本。这两个值不进本机库,总览与计划屏每次要\n");
    s.push_str("# 显示都现解析这个文件。\n");
    s.push_str(&format!(
        "standard_version = {}\n",
        quote(&f.standard_version)
    ));
    s.push_str(&format!(
        "current_version = {}\n",
        quote(&f.current_version)
    ));
    match &f.chat {
        None => {
            s.push_str("\n# 项目群没配。配了就在这里加一段 [chat](提供方 / 群号 / 同步哪些\n");
            s.push_str("# 事件),总览名片会显示群名与「通知同步中」。\n");
        }
        Some(c) => {
            s.push_str("\n[chat]\n");
            s.push_str(&format!("provider = {}\n", quote(&c.provider)));
            s.push_str(&format!("group_id = {}\n", quote(&c.group_id)));
            let items: Vec<String> = c.notify.iter().map(|n| quote(n)).collect();
            s.push_str(&format!("notify = [{}]\n", items.join(", ")));
        }
    }
    s
}

fn quote(s: &str) -> String {
    super::toml_string(s)
}
