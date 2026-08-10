//! `.bw/connectors.toml` 解析器 —— 脚本连接器的正本(主控裁决 #9:住在
//! `bw-connector`,生产 [`ConnectorEntry`] 的地方,和 `adapters::script` 挨着)。
//!
//! 移植说明(brief 要求:先查 v1 `bw-engine/src/` 有没有同族解析器,有就
//! 移植):v1 确有 `crates/bw-engine/src/connectors_file.rs`,结构与校验规则
//! (`deny_unknown_fields`、`kind` 固定词表、`[[connector]]` 数组表)整体移植
//! 到这里——本文件是那份代码的移植版,不是新写。**改动只有一处**:v1 的
//! `read()` 交回中间态 `ConnectorsFile`/`ConnectorDef`(调用方还要再转一次型
//! 成登记表用的类型);这里直接产出 `Vec<ConnectorEntry>`(种类恒
//! `ConnectorKind::Script`,`id` 现场铸造——文件本身没有 id 概念,`name` 才是
//! 这条连接器的身份,`sync_connectors_file_for` upsert 也是按 `(project_id,
//! name)`,与这里现铸 id 不冲突)。deny_unknown_fields 的 P5 事故教训
//! (`[[connectors]]` 复数拼写会静默解析成零条)原样继承。
//!
//! 格式正本:`docs/connectors-toml-format.md`。

use std::path::Path;

use bw_core::{ConnectorId, ProjectId};
use serde::Deserialize;

use crate::contract::{ConfigRef, ConnectorEntry, ConnectorKind, ProjectBinding};

/// 相对工作区根的正本文件路径,与 v1 `CONNECTORS_FILE_REL_PATH` 同值。
pub const CONNECTORS_FILE_REL_PATH: &str = ".bw/connectors.toml";

#[derive(Debug, thiserror::Error)]
pub enum ScriptSourceError {
    #[error("读取 {path} 失败:{source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("解析 {path} 失败:{source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
}

/// `.bw/connectors.toml` 的顶层结构。`deny_unknown_fields`(v1 P5 事故教训,
/// 2026-08-06:没有它,`[[connectors]]`——复数,是 `[[connector]]` 的常见笔
/// 误——会"解析成功"成零条连接器,后续 upsert 零行无报错,像是文件真的定
/// 义了零条连接器。禁未知字段把这个笔误变成响亮的解析失败。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectorsFile {
    #[serde(rename = "connector", default)]
    connector: Vec<ConnectorDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectorDef {
    name: String,
    kind: ConnectorDefKind,
    script: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    output: String,
}

/// 固定词表,目前只接受 `"script"`——v1 同款「未知类型就整份解析失败,不是
/// 静默忽略」。`github`/`codehub`/`bw`/`connector` 是 `collect_kind` 的
/// legacy inline arm,不是这份文件的词表,不在这里出现。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConnectorDefKind {
    Script,
}

/// 读 + 解析 `<workspace_root>/.bw/connectors.toml`,直接产出可注册的
/// [`ConnectorEntry`] 列表(`kind` 恒 `Script`,`binding.project` = 调用方
/// 指定的 `project`,`binding.host` = `""`,`binding.path` = 工作区根的绝对
/// 路径字符串——design §2「登记条目」的三家统一约定)。
///
/// `Ok(vec![])` 覆盖两种诚实的空:文件不存在(还没搭装置)与文件存在但零条
/// `[[connector]]`——两者都不是错误(v1 `connectors_file::read` 的
/// `Ok(None)` 语义在这里收窄成"空列表",因为调用方要的是能直接注册的条目,
/// 不需要再判一次 `Option`)。解析失败(结构错误、`kind` 不在词表、缺字
/// 段……)整份 `Err`,不写"解析一半"的中间态——`sync_connectors_file_for`
/// 的老规矩原样继承:文件必须整份解析成功才产出任何登记。
pub fn read(
    project: ProjectId,
    workspace_root: &Path,
) -> Result<Vec<ConnectorEntry>, ScriptSourceError> {
    let path = workspace_root.join(CONNECTORS_FILE_REL_PATH);
    let display = path.display().to_string();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(ScriptSourceError::Io {
                path: display,
                source: e,
            })
        }
    };
    let file: ConnectorsFile = toml::from_str(&raw).map_err(|e| ScriptSourceError::Parse {
        path: display,
        source: e,
    })?;

    let workspace_path = workspace_root.to_string_lossy().to_string();
    Ok(file
        .connector
        .into_iter()
        .map(|d| {
            let ConnectorDefKind::Script = d.kind;
            ConnectorEntry {
                id: ConnectorId::new(),
                name: d.name,
                kind: ConnectorKind::Script,
                binding: ProjectBinding {
                    project,
                    host: String::new(),
                    path: workspace_path.clone(),
                },
                config: ConfigRef::Script {
                    script: d.script,
                    command: d.command,
                    output: d.output,
                },
            }
        })
        .collect())
}
