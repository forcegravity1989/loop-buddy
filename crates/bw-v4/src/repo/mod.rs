//! 仓文件:V4 的正本。
//!
//! **仓是正本**——人、agent、第二台机器都要看的东西住在项目仓里、走 MR;库
//! 只放本机过程数据与显示缓存。这一层是读写正本的唯一入口,别处不许自己
//! 拼路径去读 `.bw/*.toml` 或 `docs/plan/*.md`。
//!
//! 三条沿用 `bw-engine` 已经立好的惯例:
//!
//! - 只读解析器一律 `deny_unknown_fields`——键名写错要当场报错,不能静默丢
//!   掉半个文件然后写进一份残缺的缓存。
//! - `Ok(None)` = 文件不存在。这是「诚实的无事发生」,不是错误。
//! - 解析失败是 `Err`,而且**绝不写半份缓存**。

pub mod issue_policy_file;
pub mod managed_file;
pub mod project_file;
pub mod release_file;
pub mod standard_file;
pub mod week_plan_file;

use std::path::{Path, PathBuf};

/// 仓文件读写的统一错误。
#[derive(Debug, thiserror::Error)]
pub enum RepoFileError {
    #[error("读写 {path} 失败:{source}")]
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
    #[error("{path} 格式不对:{why}")]
    Shape { path: String, why: String },
}

pub type Result<T> = std::result::Result<T, RepoFileError>;

/// 读一个仓内文件;不存在返回 `Ok(None)`。
pub(crate) fn read_to_string(workspace: &Path, rel: &str) -> Result<Option<String>> {
    let path = workspace.join(rel);
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(RepoFileError::Io {
            path: rel.to_string(),
            source: e,
        }),
    }
}

/// 往仓里写一个文件,父目录不存在就建。
pub(crate) fn write_file(workspace: &Path, rel: &str, body: &str) -> Result<PathBuf> {
    let path = workspace.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| RepoFileError::Io {
            path: rel.to_string(),
            source: e,
        })?;
    }
    std::fs::write(&path, body).map_err(|e| RepoFileError::Io {
        path: rel.to_string(),
        source: e,
    })?;
    Ok(path)
}

pub(crate) fn parse_toml<T: serde::de::DeserializeOwned>(rel: &str, raw: &str) -> Result<T> {
    toml::from_str(raw).map_err(|e| RepoFileError::Parse {
        path: rel.to_string(),
        source: e,
    })
}
